use crate::types::{BundleOutcome, CommitmentStage};
use anyhow::Result;
use shared::types::{ConfirmationReceiver, SlotConfirmation, SubmissionReceiver, SubmissionRecord};
use std::collections::HashMap;
use tracing::{info, warn};

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
}

impl OutcomeTracker {
    pub fn new(receiver: SubmissionReceiver, confirmation_receiver: ConfirmationReceiver) -> Self {
        Self {
            receiver,
            confirmation_receiver,
            bundles: HashMap::new(),
            latest_finalized_slot: 0,
        }
    }

    /// Registers a new bundle from a SubmissionRecord.
    /// Creates a fresh BundleOutcome and adds it to the registry.
    pub fn register(&mut self, record: SubmissionRecord) {
        let outcome = BundleOutcome::new(
            record.bundle_id.clone(),
            record.slot,
            record.leader,
            record.tip_lamports,
            record.blockhash,
            record.submitted_at,
            self.latest_finalized_slot,
        );

        info!(
            bundle_id = %record.bundle_id,
            slot = outcome.slot,
            tip_lamports = outcome.tip_lamports,
            registered_at_slot = outcome.registered_at_slot,
            "Bundle registered for tracking"
        );

        self.bundles.insert(record.bundle_id, outcome);
    }

    /// Updates a bundle's commitment stage when a stream
    /// confirmation arrives. Records timestamp and slot
    /// for latency delta calculation.
    pub fn update_stage(&mut self, bundle_id: &str, stage: CommitmentStage, slot: u64) {
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
                info!(
                    bundle_id,
                    latency_processed = ?outcome.latency_processed(),
                    latency_confirmed = ?outcome.latency_confirmed(),
                    latency_finalized = ?outcome.latency_finalized(),
                    "Bundle lifecycle complete, evicted from registry"
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

        // Match bundles whose target slot is within
        // a reasonable window of the confirmed slot
        let matching_ids: Vec<String> = self
            .bundles
            .iter()
            // .filter(|(_, outcome)| {
            //     let slot_diff = confirmation.slot as i64 - outcome.slot as i64;
            //     slot_diff >= -10 && slot_diff <= 10
            // })
            // .filter(|(_, outcome)| {
            //     let slot_diff = confirmation.slot as i64 - outcome.slot as i64;
            //     slot_diff >= 0 && slot_diff <= 32
            // })
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
            self.update_stage(&bundle_id, stage.clone(), confirmation.slot);
        }
    }
}
