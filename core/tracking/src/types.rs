use std::sync::{Arc, atomic::AtomicBool};

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
    /// Bundle never landed (leader skip, empty getBundleStatuses after polling window).
    BundleNotLanded,
    /// Unclassified failure — raw message preserved for debugging.
    Unknown(String),
}

impl FailureReason {
    /// Machine-readable failure type string for log serialization.
    pub fn failure_type(&self) -> &str {
        match self {
            Self::ExpiredBlockhash => "expired_blockhash",
            Self::FeeTooLow => "fee_too_low",
            Self::ComputeExceeded => "compute_exceeded",
            Self::BundleFailure => "bundle_failure",
            Self::BundleNotLanded => "bundle_not_landed",
            Self::Unknown(_) => "unknown",
        }
    }

    /// Human-readable recovery guidance.
    pub fn recovery_guidance(&self) -> &str {
        match self {
            Self::ExpiredBlockhash => {
                "Fetch a fresh blockhash via getLatestBlockhash at confirmed commitment and resubmit. The original blockhash exceeded the 150-slot validity window."
            }
            Self::FeeTooLow => {
                "Recalculate the tip using the Jito tip floor API (landed_tips_75th_percentile) and resubmit with a higher tip amount."
            }
            Self::ComputeExceeded => {
                "Reduce transaction compute usage or request a higher compute budget. Retry may succeed if network load has decreased."
            }
            Self::BundleFailure => {
                "Bundle was rejected by Jito. Check that the bundle contains valid transactions with correct signatures. Rebuild and resubmit."
            }
            Self::BundleNotLanded => {
                "Bundle did not land in the target leader window (likely leader skip or insufficient tip). Fetch a fresh blockhash, recalculate tip from the Jito tip floor API, and resubmit for the next leader window."
            }
            Self::Unknown(_) => {
                "Unclassified failure. Inspect the raw Jito response for details. Retry with a fresh blockhash and recalculated tip."
            }
        }
    }
}

/// Classifies where in the pipeline a failure occurred.
#[derive(Debug, Clone, PartialEq)]
pub enum FailureStage {
    /// Failure during tip calculation or blockhash fetch
    PreSubmission,
    /// Failure during Jito submission (HTTP error, rate limit)
    Submission,
    /// Failure detected by Jito (bundle rejected, execution error)
    Execution,
    /// Failure detected during confirmation tracking (timeout, not landed)
    Confirmation,
    /// Failure detected by Yellowstone transaction stream
    YellowstoneStream,
}

impl FailureStage {
    pub fn as_str(&self) -> &str {
        match self {
            Self::PreSubmission => "pre_submission",
            Self::Submission => "submission",
            Self::Execution => "execution",
            Self::Confirmation => "confirmation",
            Self::YellowstoneStream => "yellowstone_stream",
        }
    }
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
    /// Transaction signatures contained in the bundle
    pub transaction_signatures: Vec<String>,
    /// Current commitment stage
    pub stage: CommitmentStage,
    /// Source of the first processed confirmation
    pub confirmation_source: String,
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
    pub registered_at_slot: u64,
    /// If this is a retry, the original bundle ID
    pub original_bundle_id: Option<String>,
    /// Retry attempt number (0 = first submission)
    pub retry_attempt: u32,
    pub already_resolved: Arc<AtomicBool>,
}
impl BundleOutcome {
    pub fn new(
        bundle_id: String,
        slot: u64,
        leader: String,
        tip_lamports: u64,
        blockhash: String,
        transaction_signatures: Vec<String>,
        submitted_at: u64,
        registered_at_slot: u64,
        original_bundle_id: Option<String>,
        retry_attempt: u32,
        already_resolved: Arc<AtomicBool>,
    ) -> Self {
        Self {
            bundle_id,
            slot,
            leader,
            tip_lamports,
            blockhash,
            transaction_signatures,
            stage: CommitmentStage::Submitted,
            confirmation_source: "pending".to_string(),
            submitted_at,
            processed_at: None,
            confirmed_at: None,
            finalized_at: None,
            processed_slot: None,
            confirmed_slot: None,
            finalized_slot: None,
            registered_at_slot,
            original_bundle_id,
            retry_attempt,
            already_resolved,
        }
    }

    /// Returns current latency deltas between stages in seconds.
    /// None if the stage hasn't been reached yet.
    pub fn latency_processed(&self) -> Option<u64> {
        self.processed_at
            .map(|p| p.saturating_sub(self.submitted_at))
    }

    pub fn latency_confirmed(&self) -> Option<u64> {
        self.confirmed_at
            .map(|c| c.saturating_sub(self.submitted_at))
    }

    pub fn latency_finalized(&self) -> Option<u64> {
        self.finalized_at
            .map(|f| f.saturating_sub(self.submitted_at))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::AtomicBool};

    use super::BundleOutcome;

    #[test]
    fn finalized_latency_is_measured_from_submission() {
        let mut outcome = BundleOutcome::new(
            "bundle-1".to_string(),
            42,
            "leader-1".to_string(),
            1_000,
            "blockhash-1".to_string(),
            vec!["sig-1".to_string()],
            100,
            41,
            None,
            0,
            Arc::new(AtomicBool::new(false)),
        );

        outcome.finalized_at = Some(115);

        assert_eq!(outcome.latency_finalized(), Some(15));
    }
}

/// A complete lifecycle record written to the log
/// when a bundle is evicted from the registry.
#[derive(Debug, serde::Serialize)]
pub struct LifecycleEntry {
    pub bundle_id: String,
    pub slot: u64,
    pub leader: String,
    pub tip_lamports: u64,
    pub blockhash: String,
    pub confirmation_source: String,
    /// Commitment progression timestamps
    pub submitted_at: u64,
    pub processed_at: Option<u64>,
    pub confirmed_at: Option<u64>,
    pub finalized_at: Option<u64>,
    /// Commitment progression slots
    pub processed_slot: Option<u64>,
    pub confirmed_slot: Option<u64>,
    pub finalized_slot: Option<u64>,
    /// Latency deltas in seconds
    pub latency_processed_secs: Option<u64>,
    pub latency_confirmed_secs: Option<u64>,
    pub latency_finalized_secs: Option<u64>,
    /// Outcome
    pub status: String,
    /// Three-field failure classification
    pub failure_reason: Option<String>,
    pub failure_stage: Option<String>,
    pub recovery: Option<String>,
    /// Retry lineage
    pub original_bundle_id: Option<String>,
    pub retry_attempt: u32,
}
