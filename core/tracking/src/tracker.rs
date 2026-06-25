use crate::lifecycle_log::write_entry;
use crate::types::{BundleOutcome, CommitmentStage};
use crate::types::{FailureReason, FailureStage, LifecycleEntry};
use anyhow::Result;
use jito_sdk_rust::JitoJsonRpcSDK;
use shared::engine::OperationalState;
use shared::types::{
    ConfirmationReceiver, ConfirmationSender, InfraConfig, SlotConfirmation, SubmissionReceiver,
    SubmissionRecord,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{error, info, warn};

const JITO_POLL_DELAYS_SECS: &[u64] = &[5, 10, 15, 20, 30];
const RPC_POLL_START_SECS: u64 = 8;
const RPC_POLL_INTERVAL_SECS: u64 = 4;
const RPC_POLL_MAX_ATTEMPTS: u32 = 12;

fn classify_bundle_response(
    response: &serde_json::Value,
    fallback_blockhash: &str,
) -> Option<FailureReason> {
    let value = &response["result"]["value"];
    if value.is_null() || value.as_array().map(|a| a.is_empty()).unwrap_or(false) {
        if fallback_blockhash == "11111111111111111111111111111111" {
            return Some(FailureReason::ExpiredBlockhash);
        }

        return Some(FailureReason::BundleNotLanded);
    }

    let status_text = value[0]["status"].as_str().unwrap_or_default();
    let normalized = status_text.to_lowercase();

    if normalized.contains("landed")
        || normalized.contains("processed")
        || normalized.contains("confirmed")
        || normalized.contains("finalized")
        || normalized.contains("pending")
        || normalized.contains("submitted")
        || normalized.contains("received")
        || normalized.contains("success")
        || normalized.contains("ok")
    {
        return None;
    }

    if normalized.contains("expired") || normalized.contains("blockhash not found") {
        return Some(FailureReason::ExpiredBlockhash);
    }
    if normalized.contains("fee") && normalized.contains("low") {
        return Some(FailureReason::FeeTooLow);
    }
    if normalized.contains("compute") && normalized.contains("exceeded") {
        return Some(FailureReason::ComputeExceeded);
    }
    if normalized.contains("rejected")
        || normalized.contains("invalid")
        || normalized.contains("bundle failure")
        || normalized.contains("failed")
        || normalized.contains("error")
    {
        return Some(FailureReason::BundleFailure);
    }

    if let Some(err) = response["error"]["message"].as_str() {
        let err_norm = err.to_lowercase();
        if err_norm.contains("expired") {
            return Some(FailureReason::ExpiredBlockhash);
        }
        if err_norm.contains("fee") && err_norm.contains("low") {
            return Some(FailureReason::FeeTooLow);
        }
        if err_norm.contains("compute") {
            return Some(FailureReason::ComputeExceeded);
        }
        if err_norm.contains("reject")
            || err_norm.contains("invalid")
            || err_norm.contains("bundle failure")
            || err_norm.contains("fail")
        {
            return Some(FailureReason::BundleFailure);
        }

        return Some(FailureReason::Unknown(err.to_string()));
    }

    if !status_text.is_empty() {
        return Some(FailureReason::Unknown(status_text.to_string()));
    }

    Some(FailureReason::Unknown(
        "UnclassifiedJitoResponse".to_string(),
    ))
}

fn failure_stage_for(reason: &FailureReason) -> FailureStage {
    match reason {
        FailureReason::ExpiredBlockhash => FailureStage::Execution,
        FailureReason::FeeTooLow => FailureStage::Submission,
        FailureReason::ComputeExceeded => FailureStage::Execution,
        FailureReason::BundleFailure => FailureStage::Execution,
        FailureReason::BundleNotLanded => FailureStage::Confirmation,
        FailureReason::Unknown(_) => FailureStage::Confirmation,
    }
}

fn push_outcome_to_state(
    operational_state: &Arc<Mutex<OperationalState>>,
    outcome: &BundleOutcome,
    stage: &str,
    failure_reason: Option<&FailureReason>,
) {
    if let Ok(mut state) = operational_state.lock() {
        state.record_outcome(shared::types::BundleOutcomeSummary {
            bundle_id: outcome.bundle_id.clone(),
            slot: outcome.slot,
            stage: stage.to_string(),
            failure_reason: failure_reason
                .map(|r| r.failure_type().to_string())
                .unwrap_or_default(),
            failure_stage: failure_reason
                .map(|r| failure_stage_for(r).as_str().to_string())
                .unwrap_or_default(),
            recovery: failure_reason
                .map(|r| r.recovery_guidance().to_string())
                .unwrap_or_default(),
            tip_lamports: outcome.tip_lamports,
            blockhash: outcome.blockhash.clone(),
            submitted_at: outcome.submitted_at,
            original_bundle_id: outcome.original_bundle_id.clone().unwrap_or_default(),
            retry_attempt: outcome.retry_attempt,
        });
    }
}

fn lifecycle_entry_from_outcome(outcome: &BundleOutcome, status: &str) -> LifecycleEntry {
    let (failure_reason, failure_stage, recovery) = match &outcome.stage {
        CommitmentStage::Failed(reason) => (
            Some(reason.failure_type().to_string()),
            Some(failure_stage_for(reason).as_str().to_string()),
            Some(reason.recovery_guidance().to_string()),
        ),
        _ => (None, None, None),
    };

    LifecycleEntry {
        bundle_id: outcome.bundle_id.clone(),
        slot: outcome.slot,
        leader: outcome.leader.clone(),
        tip_lamports: outcome.tip_lamports,
        blockhash: outcome.blockhash.clone(),
        confirmation_source: outcome.confirmation_source.clone(),
        submitted_at: outcome.submitted_at,
        processed_at: outcome.processed_at,
        confirmed_at: outcome.confirmed_at,
        finalized_at: outcome.finalized_at,
        processed_slot: outcome.processed_slot,
        confirmed_slot: outcome.confirmed_slot,
        finalized_slot: outcome.finalized_slot,
        latency_processed_secs: outcome.latency_processed(),
        latency_confirmed_secs: outcome.latency_confirmed(),
        latency_finalized_secs: outcome.latency_finalized(),
        status: status.to_string(),
        failure_reason,
        failure_stage,
        recovery,
        original_bundle_id: outcome.original_bundle_id.clone(),
        retry_attempt: outcome.retry_attempt,
    }
}

async fn poll_jito_until_terminal(
    jito_url: &str,
    bundle_id: &str,
    blockhash: &str,
    already_resolved: &Arc<AtomicBool>,
) -> Option<FailureReason> {
    let client = JitoJsonRpcSDK::new(jito_url, None);
    let bundle_ids = vec![bundle_id.to_string()];

    for delay in JITO_POLL_DELAYS_SECS {
        tokio::time::sleep(tokio::time::Duration::from_secs(*delay)).await;

        if already_resolved.load(Ordering::SeqCst) {
            info!(bundle_id = %bundle_id, "Bundle resolved before Jito poll completed");
            return None;
        }

        match client.get_bundle_statuses(bundle_ids.clone()).await {
            Ok(response) => {
                if let Some(failure_reason) = classify_bundle_response(&response, blockhash) {
                    if matches!(failure_reason, FailureReason::BundleNotLanded) {
                        continue;
                    }

                    warn!(
                        bundle_id = %bundle_id,
                        failure_type = failure_reason.failure_type(),
                        delay_secs = delay,
                        "Jito poll observed explicit failure status"
                    );
                    return Some(failure_reason);
                }

                info!(
                    bundle_id = %bundle_id,
                    delay_secs = delay,
                    "Jito poll shows bundle pending or landed"
                );
                return None;
            }
            Err(e) => {
                warn!(
                    bundle_id = %bundle_id,
                    delay_secs = delay,
                    error = %e,
                    "Jito poll request failed, will retry"
                );
            }
        }
    }

    if already_resolved.load(Ordering::SeqCst) {
        return None;
    }

    Some(FailureReason::BundleNotLanded)
}

/// Maintains a live registry of all in-flight bundles.
/// Receives SubmissionRecords from L3 and tracks each
/// bundle through commitment stages to final outcome.
pub struct OutcomeTracker {
    /// Incoming submission records from L3
    receiver: SubmissionReceiver,
    confirmation_receiver: ConfirmationReceiver,
    confirmation_tx: ConfirmationSender,
    /// Live registry of bundles keyed by bundle ID
    bundles: HashMap<String, BundleOutcome>,
    /// Latest finalized slot seen by the tracker
    latest_finalized_slot: u64,
    jito_url: String,
    rpc_url: String,
    signature_index: HashMap<String, String>,
    operational_state: Arc<Mutex<OperationalState>>,
}

impl OutcomeTracker {
    pub fn new(
        receiver: SubmissionReceiver,
        confirmation_receiver: ConfirmationReceiver,
        confirmation_tx: ConfirmationSender,
        config: InfraConfig,
        operational_state: Arc<Mutex<OperationalState>>,
    ) -> Self {
        Self {
            receiver,
            confirmation_receiver,
            confirmation_tx,
            bundles: HashMap::new(),
            latest_finalized_slot: 0,
            jito_url: config.jito_url,
            rpc_url: config.rpc_url,
            signature_index: HashMap::new(),
            operational_state,
        }
    }

    /// Registers a new bundle from a SubmissionRecord.
    /// Creates a fresh BundleOutcome and adds it to the registry.
    pub fn register(&mut self, record: SubmissionRecord) {
        let already_resolved = Arc::new(AtomicBool::new(false));
        let already_resolved_for_poll = Arc::clone(&already_resolved);
        let already_resolved_for_rpc = Arc::clone(&already_resolved);

        let outcome = BundleOutcome::new(
            record.bundle_id.clone(),
            record.slot,
            record.leader,
            record.tip_lamports,
            record.blockhash,
            record.transaction_signatures.clone(),
            record.submitted_at,
            self.latest_finalized_slot,
            record.original_bundle_id.clone(),
            record.retry_attempt,
            already_resolved_for_poll.clone(),
        );

        for signature in &record.transaction_signatures {
            self.signature_index
                .insert(signature.clone(), record.bundle_id.clone());
        }

        info!(
            bundle_id = %record.bundle_id,
            slot = outcome.slot,
            tip_lamports = outcome.tip_lamports,
            registered_at_slot = outcome.registered_at_slot,
            retry_attempt = record.retry_attempt,
            original_bundle = ?record.original_bundle_id,
            "Bundle registered for tracking"
        );

        let bundle_id_for_poll = record.bundle_id.clone();
        let bundle_id_for_rpc = record.bundle_id.clone();
        self.bundles
            .insert(record.bundle_id.clone(), outcome.clone());

        // Jito status poll — waits through multiple intervals before classifying failure
        let jito_url = self.jito_url.clone();
        let operational_state = Arc::clone(&self.operational_state);
        let poll_blockhash = outcome.blockhash.clone();
        let poll_outcome = outcome.clone();

        tokio::spawn(async move {
            if let Some(failure_reason) = poll_jito_until_terminal(
                &jito_url,
                &bundle_id_for_poll,
                &poll_blockhash,
                &already_resolved_for_poll,
            )
            .await
            {
                if already_resolved_for_poll.load(Ordering::SeqCst) {
                    return;
                }

                let failure_stage = failure_stage_for(&failure_reason);

                warn!(
                    bundle_id = %bundle_id_for_poll,
                    failure_type = failure_reason.failure_type(),
                    failure_stage = failure_stage.as_str(),
                    "Bundle classified as failed after Jito polling window"
                );

                let mut failed_outcome = poll_outcome.clone();
                failed_outcome.stage = CommitmentStage::Failed(failure_reason.clone());

                let entry = lifecycle_entry_from_outcome(&failed_outcome, "Failed");
                write_entry(&entry);
                push_outcome_to_state(
                    &operational_state,
                    &failed_outcome,
                    "Failed",
                    Some(&failure_reason),
                );
                if let Ok(mut state) = operational_state.lock() {
                    state.bundle_resolved();
                }
                already_resolved_for_poll.store(true, Ordering::SeqCst);
            }
        });

        // RPC confirmation fallback — feeds signature-based confirmations back into L4
        let rpc_url = self.rpc_url.clone();
        let fallback_signatures = record.transaction_signatures.clone();
        let confirmation_tx = self.confirmation_tx.clone();

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(RPC_POLL_START_SECS)).await;

            let rpc_client = solana_client::rpc_client::RpcClient::new(rpc_url);
            let sigs: Vec<solana_sdk::signature::Signature> = fallback_signatures
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect();

            if sigs.is_empty() {
                return;
            }

            for attempt in 0..RPC_POLL_MAX_ATTEMPTS {
                if already_resolved_for_rpc.load(Ordering::SeqCst) {
                    return;
                }

                match rpc_client.get_signature_statuses(&sigs) {
                    Ok(response) => {
                        for (i, status_opt) in response.value.iter().enumerate() {
                            let Some(status) = status_opt else {
                                continue;
                            };

                            let commitment = if status.satisfies_commitment(
                                solana_sdk::commitment_config::CommitmentConfig::finalized(),
                            ) {
                                2u32
                            } else if status.satisfies_commitment(
                                solana_sdk::commitment_config::CommitmentConfig::confirmed(),
                            ) {
                                1u32
                            } else {
                                0u32
                            };

                            info!(
                                bundle_id = %bundle_id_for_rpc,
                                signature = %fallback_signatures[i],
                                commitment,
                                slot = status.slot,
                                attempt,
                                "RPC fallback observed signature status"
                            );

                            if let Err(e) = confirmation_tx
                                .send(SlotConfirmation {
                                    slot: status.slot,
                                    commitment,
                                    signature: Some(fallback_signatures[i].clone()),
                                })
                                .await
                            {
                                warn!(
                                    bundle_id = %bundle_id_for_rpc,
                                    error = %e,
                                    "Failed to forward RPC fallback confirmation"
                                );
                            }

                            if commitment >= 1 {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            bundle_id = %bundle_id_for_rpc,
                            attempt,
                            error = %e,
                            "RPC fallback getSignatureStatuses failed"
                        );
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(RPC_POLL_INTERVAL_SECS))
                    .await;
            }

            warn!(
                bundle_id = %bundle_id_for_rpc,
                "RPC fallback exhausted without confirmation"
            );
        });
    }

    /// Updates a bundle's commitment stage when a stream
    /// confirmation arrives. Records timestamp and slot
    /// for latency delta calculation.
    pub fn update_stage(
        &mut self,
        bundle_id: &str,
        stage: CommitmentStage,
        slot: u64,
        confirmation_source: &str,
    ) {
        if self
            .bundles
            .get(bundle_id)
            .map(|o| o.already_resolved.load(Ordering::SeqCst))
            .unwrap_or(true)
        {
            return;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let should_evict = match self.bundles.get_mut(bundle_id) {
            Some(outcome) => {
                let stage_rank = |s: &CommitmentStage| match s {
                    CommitmentStage::Submitted => 0,
                    CommitmentStage::Processed => 1,
                    CommitmentStage::Confirmed => 2,
                    CommitmentStage::Finalized => 3,
                    CommitmentStage::Failed(_) => 4,
                };

                if stage_rank(&stage) <= stage_rank(&outcome.stage)
                    && !matches!(stage, CommitmentStage::Failed(_))
                {
                    false
                } else {
                    match &stage {
                        CommitmentStage::Processed => {
                            outcome.processed_at = Some(now);
                            outcome.processed_slot = Some(slot);
                        }
                        CommitmentStage::Confirmed => {
                            outcome.confirmed_at = Some(now);
                            outcome.confirmed_slot = Some(slot);
                        }
                        CommitmentStage::Finalized => {
                            outcome.finalized_at = Some(now);
                            outcome.finalized_slot = Some(slot);
                        }
                        CommitmentStage::Failed(reason) => {
                            warn!(bundle_id, ?reason, "Bundle failed");
                        }
                        _ => {}
                    }

                    if outcome.confirmation_source == "pending"
                        || (confirmation_source == "yellowstone_stream"
                            && outcome.confirmation_source != "yellowstone_stream")
                    {
                        outcome.confirmation_source = confirmation_source.to_string();
                    }

                    outcome.stage = stage.clone();

                    info!(
                        bundle_id,
                        slot,
                        ?outcome.stage,
                        confirmation_source = outcome.confirmation_source,
                        latency_processed = ?outcome.latency_processed(),
                        latency_confirmed = ?outcome.latency_confirmed(),
                        "Bundle stage updated"
                    );

                    matches!(
                        stage,
                        CommitmentStage::Finalized | CommitmentStage::Failed(_)
                    )
                }
            }
            None => {
                warn!(bundle_id, "Received update for untracked bundle");
                false
            }
        };

        if should_evict {
            if let Some(outcome) = self.bundles.remove(bundle_id) {
                outcome.already_resolved.store(true, Ordering::SeqCst);

                for signature in &outcome.transaction_signatures {
                    self.signature_index.remove(signature);
                }

                if let Ok(mut state) = self.operational_state.lock() {
                    state.bundle_resolved();
                }

                let (status, failure_reason) = match &outcome.stage {
                    CommitmentStage::Finalized => ("Finalized", None),
                    CommitmentStage::Failed(reason) => ("Failed", Some(reason.clone())),
                    _ => ("Unknown", None),
                };

                info!(
                    bundle_id,
                    status,
                    latency_processed = ?outcome.latency_processed(),
                    latency_confirmed = ?outcome.latency_confirmed(),
                    latency_finalized = ?outcome.latency_finalized(),
                    "Bundle lifecycle complete, evicted from registry"
                );

                let entry = lifecycle_entry_from_outcome(&outcome, status);
                write_entry(&entry);

                push_outcome_to_state(
                    &self.operational_state,
                    &outcome,
                    status,
                    failure_reason.as_ref(),
                );
            }
        }
    }

    /// Main async loop. Receives SubmissionRecords from L3
    /// and registers each bundle for lifecycle tracking.
    pub async fn run(&mut self) -> Result<()> {
        info!("Outcome tracker running, awaiting submissions...");

        loop {
            tokio::select! {
                Some(record) = self.receiver.recv() => {
                    self.register(record);
                }

                Some(confirmation) = self.confirmation_receiver.recv() => {
                    self.handle_confirmation(confirmation);
                }

                else => {
                    warn!("All channels closed, tracker shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handles an incoming slot confirmation from L1.
    /// Signature-based confirmations advance bundle stages.
    /// Slot-only updates only track network finalized height.
    fn handle_confirmation(&mut self, confirmation: SlotConfirmation) {
        if confirmation.signature.is_none() {
            if confirmation.commitment == 2 && confirmation.slot > self.latest_finalized_slot {
                self.latest_finalized_slot = confirmation.slot;
            }
            return;
        }

        let stage = match confirmation.commitment {
            0 => CommitmentStage::Processed,
            1 => CommitmentStage::Confirmed,
            2 => CommitmentStage::Finalized,
            _ => return,
        };

        if matches!(stage, CommitmentStage::Finalized)
            && confirmation.slot > self.latest_finalized_slot
        {
            self.latest_finalized_slot = confirmation.slot;
        }

        if let Some(signature) = confirmation.signature.as_ref() {
            if let Some(bundle_id) = self.signature_index.get(signature).cloned() {
                let source = if confirmation.commitment == 0 {
                    "yellowstone_stream"
                } else {
                    "rpc_polling_fallback"
                };
                self.update_stage(&bundle_id, stage, confirmation.slot, source);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FailureReason, classify_bundle_response};
    use serde_json::json;

    #[test]
    fn classifies_missing_bundle_as_expired_when_fault_injected() {
        let response = json!({ "result": { "value": [] } });
        let reason = classify_bundle_response(&response, "11111111111111111111111111111111");
        assert!(matches!(reason, Some(FailureReason::ExpiredBlockhash)));
    }

    #[test]
    fn classifies_missing_bundle_as_not_landed_with_real_blockhash() {
        let response = json!({ "result": { "value": [] } });
        let reason = classify_bundle_response(&response, "some-real-blockhash");
        assert!(matches!(reason, Some(FailureReason::BundleNotLanded)));
    }

    #[test]
    fn ignores_landed_bundles() {
        let response = json!({ "result": { "value": [{ "status": "Landed" }] } });
        let reason = classify_bundle_response(&response, "some-real-blockhash");
        assert!(reason.is_none());
    }

    #[test]
    fn classifies_fee_too_low_failures() {
        let response = json!({ "result": { "value": [{ "status": "Fee too low" }] } });
        let reason = classify_bundle_response(&response, "some-real-blockhash");
        assert!(matches!(reason, Some(FailureReason::FeeTooLow)));
    }
}
