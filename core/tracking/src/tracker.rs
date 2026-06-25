use crate::lifecycle_log::write_entry;
use crate::types::{BundleOutcome, CommitmentStage};
use crate::types::{FailureReason, FailureStage, LifecycleEntry};
use anyhow::Result;
use jito_sdk_rust::JitoJsonRpcSDK;
use shared::engine::OperationalState;
use shared::types::{
    ConfirmationReceiver, InfraConfig, SlotConfirmation, SubmissionReceiver, SubmissionRecord,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{error, info, warn};

fn classify_bundle_response(
    response: &serde_json::Value,
    fallback_blockhash: &str,
) -> Option<FailureReason> {
    let value = &response["result"]["value"];
    if value.is_null() || value.as_array().map(|a| a.is_empty()).unwrap_or(false) {
        if fallback_blockhash == "11111111111111111111111111111111" {
            return Some(FailureReason::ExpiredBlockhash);
        }

        return Some(FailureReason::Unknown("BundleMissing".to_string()));
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

/// Maintains a live registry of all in-flight bundles.
/// Receives SubmissionRecords from L3 and tracks each
/// bundle through commitment stages to final outcome.
pub struct OutcomeTracker {
    /// Incoming submission records from L3
    receiver: SubmissionReceiver,
    confirmation_receiver: ConfirmationReceiver,
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
        config: InfraConfig,
        operational_state: Arc<Mutex<OperationalState>>,
    ) -> Self {
        Self {
            receiver,
            confirmation_receiver,
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
        // Set to true when bundle finalizes successfully
        let already_resolved = Arc::new(AtomicBool::new(false));
        let already_resolved_for_poll = Arc::clone(&already_resolved);

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

        let bundle_id_1 = record.bundle_id.clone();
        let bundle_id_2 = record.bundle_id.clone();

        self.bundles.insert(bundle_id_1, outcome.clone());

        // Spawn status poll
        let bundle_id = bundle_id_2;
        let jito_url = self.jito_url.clone();
        let operational_state = Arc::clone(&self.operational_state);

        tokio::spawn(async move {
            let client = JitoJsonRpcSDK::new(&jito_url, None);
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            let bundle_ids = vec![bundle_id.clone()];

            // If bundle already resolved via stream, skip failure classification
            if already_resolved_for_poll.load(Ordering::SeqCst) {
                info!(bundle_id = %bundle_id.clone(), "Bundle already resolved, skipping poll classification");
                return;
            }

            match client.get_bundle_statuses(bundle_ids).await {
                Ok(response) => {
                    if let Some(failure_reason) =
                        classify_bundle_response(&response, &outcome.blockhash)
                    {
                        let failure_stage = FailureStage::Execution;

                        warn!(
                            bundle_id = %bundle_id,
                            failure_type = failure_reason.failure_type(),
                            failure_stage = failure_stage.as_str(),
                            "Bundle classified as failed"
                        );

                        let entry = LifecycleEntry {
                            bundle_id: bundle_id.clone(),
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
                            status: "Failed".to_string(),
                            failure_reason: Some(failure_reason.failure_type().to_string()),
                            failure_stage: Some(failure_stage.as_str().to_string()),
                            recovery: Some(failure_reason.recovery_guidance().to_string()),
                            original_bundle_id: outcome.original_bundle_id.clone(),
                            retry_attempt: outcome.retry_attempt,
                        };

                        write_entry(&entry);

                        if let Ok(mut state) = operational_state.lock() {
                            state.record_outcome(shared::types::BundleOutcomeSummary {
                                bundle_id: bundle_id.clone(),
                                slot: outcome.slot,
                                stage: "Failed".to_string(),
                                failure_reason: failure_reason.failure_type().to_string(),
                                failure_stage: failure_stage.as_str().to_string(),
                                recovery: failure_reason.recovery_guidance().to_string(),
                                tip_lamports: outcome.tip_lamports,
                                blockhash: outcome.blockhash.clone(),
                                submitted_at: outcome.submitted_at,
                                original_bundle_id: outcome
                                    .original_bundle_id
                                    .clone()
                                    .unwrap_or_default(),
                                retry_attempt: outcome.retry_attempt,
                            });
                            info!(bundle_id = %bundle_id, "Failed outcome pushed to L5");
                        }
                    } else {
                        info!(bundle_id = %bundle_id, response = %response, "Bundle status polled");
                    }
                }
                Err(e) => {
                    error!(bundle_id = %bundle_id, error = %e, "Failed to poll bundle status")
                }
            }
        });

        // Spawn RPC fallback confirmation (dual-path resilience)
        // If Yellowstone/Jito haven't confirmed within 15s, poll via RPC getSignatureStatuses
        let rpc_url = self.rpc_url.clone();
        let fallback_signatures = record.transaction_signatures.clone();
        let fallback_bundle_id = record.bundle_id.clone();
        let fallback_state = Arc::clone(&self.operational_state);

        tokio::spawn(async move {
            // Wait 15s — give Yellowstone and Jito polling time to confirm first
            tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

            // Check if already confirmed by inspecting state
            {
                let state = fallback_state.lock().unwrap();
                let already_confirmed = state.recent_outcomes.iter().any(|o| {
                    o.bundle_id == fallback_bundle_id
                        && (o.stage == "Finalized" || o.stage == "Confirmed" || o.stage == "Failed")
                });
                if already_confirmed {
                    return; // Already resolved via primary path
                }
            }

            info!(
                bundle_id = %fallback_bundle_id,
                "RPC fallback: checking getSignatureStatuses"
            );

            let rpc_client = solana_client::rpc_client::RpcClient::new(rpc_url);
            let sigs: Vec<solana_sdk::signature::Signature> = fallback_signatures
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect();

            if sigs.is_empty() {
                return;
            }

            match rpc_client.get_signature_statuses(&sigs) {
                Ok(response) => {
                    for (i, status_opt) in response.value.iter().enumerate() {
                        if let Some(status) = status_opt {
                            let confirmation = if status.satisfies_commitment(
                                solana_sdk::commitment_config::CommitmentConfig::finalized(),
                            ) {
                                "Finalized"
                            } else if status.satisfies_commitment(
                                solana_sdk::commitment_config::CommitmentConfig::confirmed(),
                            ) {
                                "Confirmed"
                            } else {
                                "Processed"
                            };

                            info!(
                                bundle_id = %fallback_bundle_id,
                                signature = %fallback_signatures[i],
                                status = confirmation,
                                slot = status.slot,
                                "RPC fallback confirmed bundle"
                            );

                            // Update operational state with RPC confirmation
                            if let Ok(mut state) = fallback_state.lock() {
                                for outcome in state.recent_outcomes.iter_mut() {
                                    if outcome.bundle_id == fallback_bundle_id
                                        && outcome.stage != "Finalized"
                                        && outcome.stage != "Failed"
                                    {
                                        outcome.stage = confirmation.to_string();
                                    }
                                }
                            }
                            return; // One confirmed sig is enough
                        }
                    }
                    warn!(
                        bundle_id = %fallback_bundle_id,
                        "RPC fallback: no signature statuses found"
                    );
                }
                Err(e) => {
                    warn!(
                        bundle_id = %fallback_bundle_id,
                        error = %e,
                        "RPC fallback: getSignatureStatuses failed"
                    );
                }
            }
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let should_evict = match self.bundles.get_mut(bundle_id) {
            Some(outcome) => {
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
                    latency_processed = ?outcome.latency_processed(),
                    latency_confirmed = ?outcome.latency_confirmed(),
                    "Bundle stage updated"
                );

                // Signal eviction for terminal stages
                matches!(
                    stage,
                    CommitmentStage::Finalized | CommitmentStage::Failed(_)
                )
            }
            None => {
                warn!(bundle_id, "Received update for untracked bundle");
                false
            }
        };

        // Evict after mutable borrow is released
        if should_evict {
            if let Some(outcome) = self.bundles.remove(bundle_id) {
                outcome.already_resolved.store(true, Ordering::SeqCst);

                for signature in &outcome.transaction_signatures {
                    self.signature_index.remove(signature);
                }

                if let Ok(mut state) = self.operational_state.lock() {
                    state.bundle_resolved();
                }

                info!(
                    bundle_id,
                    latency_processed = ?outcome.latency_processed(),
                    latency_confirmed = ?outcome.latency_confirmed(),
                    latency_finalized = ?outcome.latency_finalized(),
                    "Bundle lifecycle complete, evicted from registry"
                );

                // Write to lifecycle log
                let entry = LifecycleEntry {
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
                    status: match &outcome.stage {
                        CommitmentStage::Finalized => "Finalized".to_string(),
                        CommitmentStage::Failed(_) => "Failed".to_string(),
                        _ => "Unknown".to_string(),
                    },
                    failure_reason: match &outcome.stage {
                        CommitmentStage::Failed(reason) => Some(reason.failure_type().to_string()),
                        _ => None,
                    },
                    failure_stage: match &outcome.stage {
                        CommitmentStage::Failed(_) => Some("confirmation".to_string()),
                        _ => None,
                    },
                    recovery: match &outcome.stage {
                        CommitmentStage::Failed(reason) => {
                            Some(reason.recovery_guidance().to_string())
                        }
                        _ => None,
                    },
                    original_bundle_id: outcome.original_bundle_id.clone(),
                    retry_attempt: outcome.retry_attempt,
                };

                write_entry(&entry);
            }
        }
    }

    /// Main async loop. Receives SubmissionRecords from L3
    /// and registers each bundle for lifecycle tracking.
    pub async fn run(&mut self) -> Result<()> {
        info!("Outcome tracker running, awaiting submissions...");

        loop {
            tokio::select! {
                // Incoming submission from L3
                Some(record) = self.receiver.recv() => {
                    self.register(record);
                }

                // Incoming slot confirmation from L1
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
    /// Finds any registered bundle targeting this slot
    /// and advances its commitment stage.
    fn handle_confirmation(&mut self, confirmation: SlotConfirmation) {
        let stage = match confirmation.commitment {
            0 => CommitmentStage::Processed,
            1 => CommitmentStage::Confirmed,
            2 => CommitmentStage::Finalized,
            _ => return,
        };

        // Track latest finalized slot
        if matches!(stage, CommitmentStage::Finalized) {
            if confirmation.slot > self.latest_finalized_slot {
                self.latest_finalized_slot = confirmation.slot;
            }
        }

        if let Some(signature) = confirmation.signature.as_ref() {
            if let Some(bundle_id) = self.signature_index.get(signature).cloned() {
                self.update_stage(&bundle_id, stage, confirmation.slot, "yellowstone_stream");
                return;
            }
        }

        // Match bundles whose target slot is within
        // a reasonable window of the confirmed slot
        let matching_ids: Vec<String> = self
            .bundles
            .iter()
            .filter(|(_, outcome)| {
                let slot_diff = confirmation.slot as i64 - outcome.slot as i64;
                // Only match slots at or ahead of bundle target
                // and only confirmations that arrived after registration
                slot_diff >= 0 && slot_diff <= 64 && confirmation.slot >= outcome.registered_at_slot
            })
            .map(|(id, _)| id.clone())
            .collect();

        if matching_ids.is_empty() {
            return;
        }

        for bundle_id in matching_ids {
            self.update_stage(
                &bundle_id,
                stage.clone(),
                confirmation.slot,
                "slot_heuristic",
            );
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
