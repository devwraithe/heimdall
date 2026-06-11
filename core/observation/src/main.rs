mod leader;

use dotenvy::dotenv;
use futures::{SinkExt, StreamExt};
use intelligence::consumer::IntelligenceEngine;
use shared::events::{NetworkEvent, SlotStatus, create_event_channel};
use shared::types::create_confirmation_channel;
use shared::types::create_submission_channel;
use shared::types::{SlotConfirmation, create_candidate_channel};
use solana_sdk::signature::read_keypair_file;
use std::collections::HashMap;
use std::env;
use submission::blockhash::BlockhashMode;
use submission::submitter::BundleSubmitter;
use tonic::transport::ClientTlsConfig;
use tracing::{error, info, warn};
use tracing_subscriber::FmtSubscriber;
use tracking::tracker::OutcomeTracker;
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterSlots, SubscribeRequestPing,
    subscribe_update::UpdateOneof,
};

const LEADER_WINDOW: u64 = 4;
const EVENT_CHANNEL_BUFFER: usize = 1000;
const CANDIDATE_CHANNEL_BUFFER: usize = 100;
const SUBMISSION_CHANNEL_BUFFER: usize = 100;
const CONFIRMATION_CHANNEL_BUFFER: usize = 1000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    // Init structuredd logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let endpoint = env::var("GRPC_ENDPOINT")?;
    let token = env::var("GRPC_X_TOKEN")?;
    let rpc_url = env::var("RPC_ENDPOINT")?;
    let jito_url = env::var("JITO_URL")?;
    let keypair_path =
        env::var("KEYPAIR_PATH").unwrap_or_else(|_| "~/.config/solana/id.json".to_string());
    let rpc_url_clone = rpc_url.clone();

    // Load keypair
    let keypair = read_keypair_file(&keypair_path)
        .map_err(|e| anyhow::anyhow!("Failed to load keypair: {}", e))?;

    // Create channels
    let (event_tx, event_rx) = create_event_channel(EVENT_CHANNEL_BUFFER);
    let (candidate_tx, mut candidate_rx) = create_candidate_channel(CANDIDATE_CHANNEL_BUFFER);
    let (submission_tx, submission_rx) = create_submission_channel(SUBMISSION_CHANNEL_BUFFER);
    let (confirmation_tx, confirmation_rx) =
        create_confirmation_channel(CONFIRMATION_CHANNEL_BUFFER);

    // Spawn L2 intelligence engine
    tokio::spawn(async move {
        let mut engine = IntelligenceEngine::new(event_rx, candidate_tx);
        if let Err(e) = engine.run().await {
            error!(error = %e, "Intelligence engine error");
        }
    });

    // Spawn L3 bundle submitter
    tokio::spawn(async move {
        let submitter = BundleSubmitter::new(&rpc_url, &jito_url, keypair, BlockhashMode::Normal);

        while let Some(candidate) = candidate_rx.recv().await {
            match submitter.submit(candidate.slot, candidate.leader).await {
                Ok(record) => {
                    info!(
                        bundle_id = %record.bundle_id,
                        slot = record.slot,
                        tip_lamports = record.tip_lamports,
                        "Submission record created"
                    );

                    // Forward record to L4
                    if let Err(e) = submission_tx.send(record).await {
                        error!(error = %e, "Failed to send record to L4");
                    }
                }
                Err(e) => {
                    error!(error = %e, "Bundle submission failed");
                }
            }
        }
    });

    // Spawn L4 outcome tracker
    tokio::spawn(async move {
        let mut tracker = OutcomeTracker::new(submission_rx, confirmation_rx);
        if let Err(e) = tracker.run().await {
            error!(error = %e, "Outcome tracker error");
        }
    });

    info!(endpoint = %endpoint, "Connecting to Yellowstone");

    let tls_config = ClientTlsConfig::new().with_native_roots();
    let mut client = GeyserGrpcClient::build_from_shared(endpoint)?
        .x_token(Some(token))?
        .tls_config(tls_config)?
        .connect()
        .await?;

    info!("Connected successfully");

    // Build slots subscription request
    let mut slots = HashMap::new();
    slots.insert(
        "slots".to_string(),
        SubscribeRequestFilterSlots {
            filter_by_commitment: None,
            interslot_updates: None,
        },
    );

    let request = SubscribeRequest {
        slots,
        ..Default::default()
    };

    // Open stream
    let (mut sink, mut stream) = client.subscribe_with_request(Some(request)).await?;

    info!("Slot stream open, listening...");

    let mut leader_schedule: Option<leader::LeaderSchedule> = None;

    // Consume stream
    while let Some(message) = stream.next().await {
        match message {
            Ok(msg) => {
                match msg.update_oneof {
                    Some(UpdateOneof::Ping(_)) => {
                        // Reply with pong to keep connection alive
                        sink.send(SubscribeRequest {
                            ping: Some(SubscribeRequestPing { id: 1 }),
                            ..Default::default()
                        })
                        .await?;
                        info!("Ping received, pong sent");
                    }
                    Some(UpdateOneof::Slot(slot)) => {
                        let status = SlotStatus::from_u32(slot.status as u32);

                        // Emit slot event
                        event_tx
                            .send(NetworkEvent::SlotUpdate {
                                slot: slot.slot,
                                status: status.clone(),
                            })
                            .await?;

                        // Forward slot confirmation to L4
                        if let Err(e) = confirmation_tx
                            .send(SlotConfirmation {
                                slot: slot.slot,
                                commitment: slot.status as u32,
                            })
                            .await
                        {
                            warn!(error = %e, "Failed to send slot confirmation to L4");
                        }

                        // Fetch leader schedule once on first processed slot
                        if leader_schedule.is_none() && matches!(status, SlotStatus::Processed) {
                            match leader::LeaderSchedule::fetch(&rpc_url_clone, slot.slot) {
                                Ok(schedule) => {
                                    info!("Leader schedule ready");
                                    leader_schedule = Some(schedule);
                                }
                                Err(e) => error!(error = %e, "Failed to fetch leader schedule"),
                            }
                        }

                        // Emit leader window events on each processed slot
                        if matches!(status, SlotStatus::Processed) {
                            if let Some(ref schedule) = leader_schedule {
                                let upcoming =
                                    schedule.get_upcoming_leaders(slot.slot, LEADER_WINDOW);
                                for (upcoming_slot, leader) in upcoming {
                                    event_tx
                                        .send(NetworkEvent::LeaderWindow {
                                            slot: upcoming_slot,
                                            leader: leader.to_string(),
                                        })
                                        .await?;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => error!("Error receiving slot update: {:?}", e),
        }
    }

    warn!("Stream ended unexpectedly");
    Ok(())
}
