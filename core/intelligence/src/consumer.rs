use anyhow::Result;
pub use shared::types::TransactionCandidate;
use shared::{
    events::{EventReceiver, NetworkEvent},
    types::CandidateSender,
};
use std::collections::HashSet;
use tracing::{info, warn};

pub struct IntelligenceEngine {
    receiver: EventReceiver,
    candidate_sender: CandidateSender,
    seen_candidates: HashSet<u64>,
}

impl IntelligenceEngine {
    pub fn new(receiver: EventReceiver, candidate_sender: CandidateSender) -> Self {
        Self {
            receiver,
            candidate_sender,
            seen_candidates: HashSet::new(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Intelligence engine running, consuming L1 events...");

        while let Some(event) = self.receiver.recv().await {
            match event {
                NetworkEvent::LeaderWindow { slot, leader } => {
                    if self.seen_candidates.insert(slot) {
                        let candidate = TransactionCandidate::new(slot, leader.clone());
                        info!(
                            slot = candidate.slot,
                            leader = %candidate.leader,
                            created_at = candidate.created_at,
                            "Transaction candidate created"
                        );
                        // Emit candidate to L3
                        if let Err(e) = self.candidate_sender.send(candidate).await {
                            warn!(error = %e, "Failed to send candidate to L3");
                        }
                    }
                }
                NetworkEvent::SlotUpdate { slot, status } => {
                    info!(slot, ?status, "Slot update received");
                }
                NetworkEvent::BlockObserved { slot } => {
                    warn!(slot, "Block observed (not yet implemented)");
                }
                NetworkEvent::TransactionObserved { signature } => {
                    warn!(%signature, "Transaction observed (not yet implemented)");
                }
            }
        }

        warn!("L1 event channel closed");
        Ok(())
    }
}
