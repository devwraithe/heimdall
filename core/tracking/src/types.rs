/// Classifies why a bundle failed.
/// Used by L4 to categorise outcomes and
/// by L6's AI agent to reason about retries.
#[derive(Debug, Clone, PartialEq)]
pub enum FailureReason {
    /// Blockhash was too old when bundle reached the leader.
    /// AI agent must refresh blockhash before retrying.
    ExpiredBlockhash,
    /// Tip amount was too low for inclusion.
    /// AI agent must recalculate tip before retrying.
    FeeTooLow,
    /// Transaction exceeded compute budget.
    ComputeExceeded,
    /// Jito-level bundle rejection.
    BundleFailure,
    /// Unclassified failure — raw message preserved for debugging.
    Unknown(String),
}

/// Represents a bundle's current position
/// in the Solana commitment pipeline
#[derive(Debug, Clone, PartialEq)]
pub enum CommitmentStage {
    /// Bundle submitted to Jito, awaiting network observation
    Submitted,
    /// Bundle seen by the network, not yet confirmed
    Processed,
    /// 2/3 stake voted on the slot containing the bundle
    Confirmed,
    /// Slot containing bundle is permanent and irreversible
    Finalized,
    /// Bundle failed at some stage
    Failed(FailureReason),
}

/// Complete lifecycle record of a submitted bundle.
/// Populated by L3's SubmissionRecord at creation,
/// updated by L4 as commitment stages progress.
#[derive(Debug, Clone)]
pub struct BundleOutcome {
    /// Jito bundle ID
    pub bundle_id: String,
    /// Target slot at submission time
    pub slot: u64,
    /// Validator identity that led the target slot
    pub leader: String,
    /// Tip paid in lamports
    pub tip_lamports: u64,
    /// Blockhash used at bundle construction
    pub blockhash: String,
    /// Current commitment stage
    pub stage: CommitmentStage,
    /// Unix timestamp of submission
    pub submitted_at: u64,
    /// Unix timestamp when bundle was processed
    pub processed_at: Option<u64>,
    /// Unix timestamp when bundle was confirmed
    pub confirmed_at: Option<u64>,
    /// Unix timestamp when bundle was finalized
    pub finalized_at: Option<u64>,
    /// Slot number at processed stage
    pub processed_slot: Option<u64>,
    /// Slot number at confirmed stage
    pub confirmed_slot: Option<u64>,
    /// Slot number at finalized stage
    pub finalized_slot: Option<u64>,
    /// The latest finalized slot when this bundle was registered.
    /// Used to ignore stale confirmations from before registration.
    pub registered_at_slot: u64,
}
impl BundleOutcome {
    /// Creates a fresh BundleOutcome from a SubmissionRecord.
    /// All commitment stage fields start as None —
    /// populated by L4 as the bundle progresses.
    pub fn new(
        bundle_id: String,
        slot: u64,
        leader: String,
        tip_lamports: u64,
        blockhash: String,
        submitted_at: u64,
        registered_at_slot: u64,
    ) -> Self {
        Self {
            bundle_id,
            slot,
            leader,
            tip_lamports,
            blockhash,
            stage: CommitmentStage::Submitted,
            submitted_at,
            processed_at: None,
            confirmed_at: None,
            finalized_at: None,
            processed_slot: None,
            confirmed_slot: None,
            finalized_slot: None,
            registered_at_slot,
        }
    }

    /// Returns current latency deltas between stages in seconds.
    /// None if the stage hasn't been reached yet.
    pub fn latency_processed(&self) -> Option<u64> {
        self.processed_at
            .map(|p| p.saturating_sub(self.submitted_at))
    }

    pub fn latency_confirmed(&self) -> Option<u64> {
        self.processed_at
            .map(|p| p.saturating_sub(self.submitted_at))
    }

    pub fn latency_finalized(&self) -> Option<u64> {
        self.finalized_at
            .zip(self.confirmed_at)
            .map(|(f, c)| f.saturating_sub(c))
    }
}
