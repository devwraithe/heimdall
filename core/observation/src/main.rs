mod leader;

use dotenvy::dotenv;
use futures::{SinkExt, StreamExt};
use intelligence::consumer::IntelligenceEngine;
use shared::engine::OperationalState;
use shared::events::{NetworkEvent, SlotStatus, create_event_channel};
use shared::types::{
    InfraConfig, SlotConfirmation, SubmissionRecord, TipUpdate, create_candidate_channel,
    create_confirmation_channel, create_submission_channel, create_tip_channel,
};
use solana_sdk::signature::{Signature, read_keypair_file};
use state::heimdall::{
    RetryRequest, operational_state_service_server::OperationalStateServiceServer,
};
use state::server::StateServer;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use submission::{blockhash::BlockhashMode, submitter::BundleSubmitter};
use tokio::sync::mpsc;
use tonic::transport::{ClientTlsConfig, Server};
use tracing::{error, info, warn};
use tracing_subscriber::FmtSubscriber;
use tracking::tracker::OutcomeTracker;
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::geyser::{
    CommitmentLevel, SubscribeRequest, SubscribeRequestFilterSlots,
    SubscribeRequestFilterTransactions, SubscribeRequestPing, subscribe_update::UpdateOneof,
};

// Constants
const EVENT_CHANNEL_BUFFER: usize = 1000;
const CANDIDATE_CHANNEL_BUFFER: usize = 100;
const SUBMISSION_CHANNEL_BUFFER: usize = 100;
const CONFIRMATION_CHANNEL_BUFFER: usize = 1000;
const TIP_CHANNEL_BUFFER: usize = 100;
const MAX_RECENT_SUBMISSIONS: usize = 128;
const GRPC_PORT: u16 = 50051;

async fn watch_bundle_signatures(
    endpoint: String,
    token: String,
    bundle_id: String,
    signatures: Vec<String>,
    confirmation_tx: tokio::sync::mpsc::Sender<SlotConfirmation>,
) -> anyhow::Result<()> {
    let tls_config = ClientTlsConfig::new().with_native_roots();
    let mut client = GeyserGrpcClient::build_from_shared(endpoint)?
        .x_token(Some(token))?
        .tls_config(tls_config)?
        .connect()
        .await?;

    let mut transactions_status = HashMap::new();
    for (idx, signature) in signatures.iter().enumerate() {
        transactions_status.insert(
            format!("{bundle_id}-{idx}"),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: Some(signature.clone()),
                ..Default::default()
            },
        );
    }

    let request = SubscribeRequest {
        transactions_status,
        commitment: Some(CommitmentLevel::Processed as i32),
        ..Default::default()
    };

    let (mut sink, mut stream) = client.subscribe_with_request(Some(request)).await?;

    while let Some(message) = stream.next().await {
        match message {
            Ok(msg) => match msg.update_oneof {
                Some(UpdateOneof::Ping(_)) => {
                    sink.send(SubscribeRequest {
                        ping: Some(SubscribeRequestPing { id: 1 }),
                        ..Default::default()
                    })
                    .await?;
                }
                Some(UpdateOneof::Transaction(tx)) => {
                    let Some(info) = tx.transaction else {
                        continue;
                    };

                    let Ok(signature) = Signature::try_from(info.signature.as_slice()) else {
                        continue;
                    };

                    let signature = signature.to_string();
                    if signatures.iter().any(|watched| watched == &signature) {
                        if let Err(e) = confirmation_tx.try_send(SlotConfirmation {
                            slot: tx.slot,
                            commitment: 0,
                            signature: Some(signature.clone()),
                        }) {
                            warn!(bundle_id = %bundle_id, error = %e, "Failed to forward Yellowstone transaction confirmation");
                        } else {
                            info!(
                                bundle_id = %bundle_id,
                                slot = tx.slot,
                                signature = %signature,
                                "Yellowstone transaction confirmation observed"
                            );
                        }
                        break;
                    }
                }
                Some(UpdateOneof::TransactionStatus(tx)) => {
                    let Ok(signature) = Signature::try_from(tx.signature.as_slice()) else {
                        continue;
                    };

                    let signature = signature.to_string();
                    if signatures.iter().any(|watched| watched == &signature) {
                        if let Err(e) = confirmation_tx.try_send(SlotConfirmation {
                            slot: tx.slot,
                            commitment: 0,
                            signature: Some(signature.clone()),
                        }) {
                            warn!(bundle_id = %bundle_id, error = %e, "Failed to forward Yellowstone transaction status");
                        } else {
                            info!(
                                bundle_id = %bundle_id,
                                slot = tx.slot,
                                signature = %signature,
                                "Yellowstone transaction status observed"
                            );
                        }
                        break;
                    }
                }
                _ => {}
            },
            Err(e) => {
                warn!(bundle_id = %bundle_id, error = %e, "Yellowstone transaction watcher ended");
                break;
            }
        }
    }

    #[allow(unreachable_code)]
    Ok(())
}

async fn publish_submission_record(
    record: SubmissionRecord,
    recent_submissions: &mut HashMap<String, SubmissionRecord>,
    submission_tx: &mpsc::Sender<SubmissionRecord>,
    tip_tx: &mpsc::Sender<TipUpdate>,
    confirmation_tx: &mpsc::Sender<SlotConfirmation>,
    operational_state: &Arc<Mutex<OperationalState>>,
    endpoint: &str,
    token: &str,
) {
    info!(
        bundle_id = %record.bundle_id,
        slot = record.slot,
        tip_lamports = record.tip_lamports,
        "Submission record created"
    );

    recent_submissions.insert(record.bundle_id.clone(), record.clone());
    if recent_submissions.len() > MAX_RECENT_SUBMISSIONS {
        if let Some(oldest_id) = recent_submissions
            .iter()
            .min_by_key(|(_, r)| r.submitted_at)
            .map(|(id, _)| id.clone())
        {
            recent_submissions.remove(&oldest_id);
        }
    }

    if let Ok(mut state) = operational_state.lock() {
        state.bundle_submitted();
    }

    if let Err(e) = submission_tx.send(record.clone()).await {
        error!(error = %e, "Failed to send record to L4");
    }

    if let Err(e) = tip_tx
        .send(TipUpdate {
            median_lamports: record.tip_lamports,
        })
        .await
    {
        warn!(error = %e, "Failed to send tip update to L5");
    }

    let watcher_endpoint = endpoint.to_string();
    let watcher_token = token.to_string();
    let bundle_id = record.bundle_id.clone();
    let signatures = record.transaction_signatures.clone();
    let confirmation_tx = confirmation_tx.clone();

    tokio::spawn(async move {
        if let Err(e) = watch_bundle_signatures(
            watcher_endpoint,
            watcher_token,
            bundle_id.clone(),
            signatures,
            confirmation_tx,
        )
        .await
        {
            warn!(bundle_id = %bundle_id, error = %e, "Yellowstone signature watcher failed");
        }
    });
}

fn select_retry_record(
    retry: &RetryRequest,
    recent_submissions: &HashMap<String, SubmissionRecord>,
) -> Option<SubmissionRecord> {
    retry
        .failed_bundles
        .iter()
        .filter_map(|failed| recent_submissions.get(&failed.bundle_id))
        .max_by_key(|record| record.submitted_at)
        .cloned()
}

#[tokio::main]
// #[allow(unreachable_code)]
async fn main() -> anyhow::Result<()> {
    // Load .env file from root
    dotenv().ok();

    // Initialize structured logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Environment variables
    let endpoint = env::var("GRPC_ENDPOINT")?;
    let x_token = env::var("GRPC_X_TOKEN")?;
    let rpc_url = env::var("RPC_ENDPOINT")?;
    let jito_url = env::var("JITO_URL")?;
    let default_keypair_path = env::var("DEFAULT_KEYPAIR_PATH")?;
    let keypair_path = env::var("KEYPAIR_PATH").unwrap_or_else(|_| default_keypair_path.clone());
    let leader_window: u64 = env::var("LEADER_WINDOW")?.parse()?;
    let rpc_url_clone = rpc_url.clone();

    // Load keypair
    let keypair = read_keypair_file(&keypair_path)
        .map_err(|e| anyhow::anyhow!("Failed to load keypair: {}", e))?;

    // Create shared operational state
    let operational_state = Arc::new(Mutex::new(OperationalState::new()));
    let state_for_grpc = Arc::clone(&operational_state);
    let state_for_tip = Arc::clone(&operational_state);
    let state_for_l4 = Arc::clone(&operational_state);
    let state_for_slots = Arc::clone(&operational_state);
    let state_for_l3 = Arc::clone(&operational_state);

    // Create channels
    let (event_tx, event_rx) = create_event_channel(EVENT_CHANNEL_BUFFER);
    let (candidate_tx, mut candidate_rx) = create_candidate_channel(CANDIDATE_CHANNEL_BUFFER);
    let (submission_tx, submission_rx) = create_submission_channel(SUBMISSION_CHANNEL_BUFFER);
    let (confirmation_tx, confirmation_rx) =
        create_confirmation_channel(CONFIRMATION_CHANNEL_BUFFER);
    let (tip_tx, mut tip_rx) = create_tip_channel(TIP_CHANNEL_BUFFER);
    // Retry channel
    let (retry_tx, mut retry_rx) = mpsc::channel::<RetryRequest>(32);

    // Spawn L2 intelligence engine
    tokio::spawn(async move {
        let mut engine = IntelligenceEngine::new(event_rx, candidate_tx);
        if let Err(e) = engine.run().await {
            error!(error = %e, "Intelligence engine error");
        }
    });

    let jito_url_for_l3 = jito_url.clone();
    let jito_url_for_l4 = jito_url.clone();
    let rpc_url_for_l4 = rpc_url.clone();
    let endpoint_for_l3 = endpoint.clone();
    let token_for_l3 = x_token.clone();
    let confirmation_tx_for_l3 = confirmation_tx.clone();

    // Spawn L3 bundle submitter
    tokio::spawn(async move {
        let blockhash_mode = match std::env::var("BLOCKHASH_MODE").as_deref() {
            Ok("fault_injected") => BlockhashMode::FaultInjected,
            _ => BlockhashMode::Normal,
        };
        let submitter = BundleSubmitter::new(&rpc_url, &jito_url_for_l3, keypair, blockhash_mode);
        let mut recent_submissions: HashMap<String, SubmissionRecord> = HashMap::new();
        let mut retry_counts: HashMap<String, u32> = HashMap::new();

        const MAX_RETRY_ATTEMPTS: u32 = 4;
        const BACKOFF_BASE_MS: u64 = 2000;

        loop {
            tokio::select! {
                Some(candidate) = candidate_rx.recv() => {
                    match submitter.submit_with_options(
                        candidate.slot,
                        candidate.leader,
                        None,
                        false,
                    ).await {
                        Ok(record) => {
                            publish_submission_record(
                                record,
                                &mut recent_submissions,
                                &submission_tx,
                                &tip_tx,
                                &confirmation_tx_for_l3,
                                &state_for_l3,
                                &endpoint_for_l3,
                                &token_for_l3,
                            )
                            .await;
                        }
                        Err(e) => {
                            error!(error = %e, "Bundle submission failed");
                        }
                    }
                }

                Some(retry) = retry_rx.recv() => {
                    info!(
                        refresh_blockhash = retry.refresh_blockhash,
                        suggested_tip = retry.suggested_tip_lamports,
                        failure_classification = %retry.failure_classification,
                        confidence = retry.confidence,
                        failed_bundle_count = retry.failed_bundles.len(),
                        "Retry request received from L6"
                    );

                    if let Some(record_to_retry) = select_retry_record(&retry, &recent_submissions) {
                        let original_bundle_id = record_to_retry
                            .original_bundle_id
                            .clone()
                            .unwrap_or_else(|| record_to_retry.bundle_id.clone());
                        let retry_key = original_bundle_id.clone();
                        let attempt = retry_counts.entry(retry_key.clone()).or_insert(0);
                        let next_attempt = *attempt + 1;

                        if next_attempt > MAX_RETRY_ATTEMPTS {
                            warn!(
                                bundle_id = %record_to_retry.bundle_id,
                                original_bundle = %original_bundle_id,
                                attempts = next_attempt,
                                max = MAX_RETRY_ATTEMPTS,
                                "Max retry attempts reached for original bundle, dropping retry chain"
                            );
                            retry_counts.remove(&retry_key);
                            if let Ok(mut state) = state_for_l3.lock() {
                                state.record_retry_exhausted();
                            }
                            continue;
                        }

                        *attempt = next_attempt;

                        // Record retry attempt in operational state
                        if let Ok(mut state) = state_for_l3.lock() {
                            state.record_retry();
                        }

                        let backoff_ms = BACKOFF_BASE_MS * (1u64 << (*attempt - 1));
                        info!(
                            bundle_id = %record_to_retry.bundle_id,
                            original_bundle = %original_bundle_id,
                            attempt = *attempt,
                            max_attempts = MAX_RETRY_ATTEMPTS,
                            backoff_ms,
                            "Exponential backoff before retry"
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;

                        let current_attempt = *attempt;
                        match submitter.submit_with_options(
                            record_to_retry.slot,
                            record_to_retry.leader.clone(),
                            Some(retry.suggested_tip_lamports),
                            retry.refresh_blockhash,
                        ).await {
                            Ok(record) => {
                                // Tag record with retry lineage
                                let record = record.as_retry(original_bundle_id.clone(), current_attempt);
                                info!(
                                    bundle_id = %record.bundle_id,
                                    original_bundle = %original_bundle_id,
                                    attempt = current_attempt,
                                    "Retry submission succeeded"
                                );
                                if let Ok(mut state) = state_for_l3.lock() {
                                    state.record_retry_success();
                                }
                                retry_counts.remove(&retry_key);
                                publish_submission_record(
                                    record,
                                    &mut recent_submissions,
                                    &submission_tx,
                                    &tip_tx,
                                    &confirmation_tx_for_l3,
                                    &state_for_l3,
                                    &endpoint_for_l3,
                                    &token_for_l3,
                                )
                                .await;
                            }
                            Err(e) => {
                                error!(
                                    error = %e,
                                    bundle_id = %record_to_retry.bundle_id,
                                    attempt = current_attempt,
                                    "Retry submission failed"
                                );
                            }
                        }
                    } else {
                        warn!(
                            failed_bundle_count = retry.failed_bundles.len(),
                            "Retry request received but no cached submission matched the failed bundle"
                        );
                    }
                }
            }
        }
    });

    // Spawn L4 outcome tracker
    let confirmation_tx_for_l4 = confirmation_tx.clone();
    tokio::spawn(async move {
        let mut tracker = OutcomeTracker::new(
            submission_rx,
            confirmation_rx,
            confirmation_tx_for_l4,
            InfraConfig {
                jito_url: jito_url_for_l4,
                rpc_url: rpc_url_for_l4,
            },
            Arc::clone(&state_for_l4),
        );
        if let Err(e) = tracker.run().await {
            error!(error = %e, "Outcome tracker error");
        }
    });

    // Spawn gRPC server
    let grpc_state = Arc::clone(&state_for_grpc);
    tokio::spawn(async move {
        let addr = format!("0.0.0.0:{}", GRPC_PORT).parse().unwrap();
        let server = StateServer::new(grpc_state, retry_tx.clone());

        info!(port = GRPC_PORT, "L5 gRPC server starting");

        Server::builder()
            .add_service(OperationalStateServiceServer::new(server))
            .serve(addr)
            .await
            .expect("L5 gRPC server failed");
    });

    // Spawn tip update consumer
    let tip_state = Arc::clone(&state_for_tip);
    tokio::spawn(async move {
        while let Some(update) = tip_rx.recv().await {
            if let Ok(mut state) = tip_state.lock() {
                state.update_tip(update.median_lamports);
            }
        }
    });

    // Yellowstone tls and reconnection config
    let tls_config = ClientTlsConfig::new().with_native_roots();
    let reconnect_max = std::time::Duration::from_secs(30);
    let mut reconnect_delay = std::time::Duration::from_secs(1);

    loop {
        // Yellowstone gRPC client connection
        info!(endpoint = %endpoint, reconnect_delay_secs = reconnect_delay.as_secs(), "Connecting to Yellowstone");

        let mut client = match GeyserGrpcClient::build_from_shared(endpoint.clone())?
            .x_token(Some(&x_token))?
            .tls_config(tls_config.clone())?
            .connect()
            .await
        {
            Ok(client) => client,
            Err(e) => {
                error!(error = %e, "Failed to connect to Yellowstone");
                tokio::time::sleep(reconnect_delay).await;
                reconnect_delay = std::time::Duration::from_secs(
                    (reconnect_delay.as_secs().saturating_mul(2)).min(reconnect_max.as_secs()),
                );
                continue;
            }
        };

        info!("Connected successfully");

        // Monitor live slots from gRPC streaming
        let mut slots = HashMap::new();
        let request_filters_slots = SubscribeRequestFilterSlots {
            filter_by_commitment: None,
            interslot_updates: None,
        };
        slots.insert("slots".to_string(), request_filters_slots);

        let request = SubscribeRequest {
            slots,
            ..Default::default()
        };

        let (mut sink, mut stream) = match client.subscribe_with_request(Some(request)).await {
            Ok(pair) => pair,
            Err(e) => {
                error!(error = %e, "Failed to open Yellowstone slot stream");
                tokio::time::sleep(reconnect_delay).await;
                reconnect_delay = std::time::Duration::from_secs(
                    (reconnect_delay.as_secs().saturating_mul(2)).min(reconnect_max.as_secs()),
                );
                continue;
            }
        };

        reconnect_delay = std::time::Duration::from_secs(1);
        info!("Slot stream open, listening...");

        // Monitor leader schedule data
        let mut leader_schedule: Option<leader::LeaderSchedule> = None;

        loop {
            match stream.next().await {
                Some(Ok(msg)) => match msg.update_oneof {
                    // Intercept ping from gRPC and send pong back
                    Some(UpdateOneof::Ping(_)) => {
                        if let Err(e) = sink
                            .send(SubscribeRequest {
                                ping: Some(SubscribeRequestPing { id: 1 }),
                                ..Default::default()
                            })
                            .await
                        {
                            error!(error = %e, "Failed to answer Yellowstone ping");
                            break;
                        }
                        info!("Ping received, pong sent");
                    }
                    Some(UpdateOneof::Slot(slot)) => {
                        let status = SlotStatus::from_u32(slot.status as u32);

                        if let Err(e) = event_tx.try_send(NetworkEvent::SlotUpdate {
                            slot: slot.slot,
                            status: status.clone(),
                        }) {
                            warn!(error = %e, "Event channel full, dropping slot update");
                        }

                        if let Ok(mut state) = state_for_slots.lock() {
                            state.update_slot(
                                slot.slot,
                                slot.status == 2, // 2 = Finalized
                            );
                        }

                        if let Err(e) = confirmation_tx.try_send(SlotConfirmation {
                            slot: slot.slot,
                            commitment: slot.status as u32,
                            signature: None,
                        }) {
                            warn!(error = %e, "Failed to send slot confirmation to L4");
                        }

                        if leader_schedule.is_none() && matches!(status, SlotStatus::Processed) {
                            match leader::LeaderSchedule::fetch(&rpc_url_clone, slot.slot) {
                                Ok(schedule) => {
                                    info!("Leader schedule ready");
                                    leader_schedule = Some(schedule);
                                }
                                Err(e) => error!(error = %e, "Failed to fetch leader schedule"),
                            }
                        }

                        if matches!(status, SlotStatus::Processed) {
                            if let Some(ref schedule) = leader_schedule {
                                let upcoming =
                                    schedule.get_upcoming_leaders(slot.slot, leader_window);
                                for (upcoming_slot, leader) in upcoming {
                                    if let Err(e) = event_tx.try_send(NetworkEvent::LeaderWindow {
                                        slot: upcoming_slot,
                                        leader: leader.to_string(),
                                    }) {
                                        warn!(error = %e, "Event channel full, dropping leader window");
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                },
                Some(Err(e)) => {
                    error!(error = %e, "Error receiving slot update");
                    break;
                }
                None => {
                    warn!("Stream ended unexpectedly");
                    break;
                }
            }
        }

        warn!(
            reconnect_delay_secs = reconnect_delay.as_secs(),
            "Yellowstone stream disconnected, reconnecting"
        );

        tokio::time::sleep(reconnect_delay).await;
        reconnect_delay = std::time::Duration::from_secs(
            (reconnect_delay.as_secs().saturating_mul(2)).min(reconnect_max.as_secs()),
        );
    }
}
