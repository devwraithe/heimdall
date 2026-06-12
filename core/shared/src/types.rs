use tokio::sync::mpsc;

pub type CandidateSender = mpsc::Sender<TransactionCandidate>;
pub type CandidateReceiver = mpsc::Receiver<TransactionCandidate>;

/// Represents a transaction candidate produced by L2
/// and passed downstream to L3 via L5
#[derive(Debug, Clone)]
pub struct TransactionCandidate {
    /// The slot this candidate was created at
    pub slot: u64,
    /// The validator leading this slot
    pub leader: String,
    /// Unix timestamp when candidate was created
    pub created_at: u64,
}

impl TransactionCandidate {
    pub fn new(slot: u64, leader: String) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            slot,
            leader,
            created_at,
        }
    }
}

pub fn create_candidate_channel(buffer: usize) -> (CandidateSender, CandidateReceiver) {
    mpsc::channel(buffer)
}

#[derive(Debug, Clone)]
pub struct SubmissionRecord {
    /// Jito bundle ID returned after submission
    pub bundle_id: String,
    /// Target slot this bundle was submitted for
    pub slot: u64,
    /// Validator identity leading the target slot
    pub leader: String,
    /// Tip amount paid in lamports
    pub tip_lamports: u64,
    /// Blockhash used when constructing the bundle
    pub blockhash: String,
    /// Unix timestamp of submission
    pub submitted_at: u64,
}

impl SubmissionRecord {
    pub fn new(
        bundle_id: String,
        slot: u64,
        leader: String,
        tip_lamports: u64,
        blockhash: String,
    ) -> Self {
        let submitted_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            bundle_id,
            slot,
            leader,
            tip_lamports,
            blockhash,
            submitted_at,
        }
    }
}

pub type SubmissionSender = mpsc::Sender<SubmissionRecord>;
pub type SubmissionReceiver = mpsc::Receiver<SubmissionRecord>;

pub fn create_submission_channel(buffer: usize) -> (SubmissionSender, SubmissionReceiver) {
    mpsc::channel(buffer)
}

/// Carries a slot update from L1 to L4 for bundle confirmation.
/// Uses raw commitment u32 to avoid circular dependency with tracking crate.
/// L4 converts to CommitmentStage internally.
#[derive(Debug, Clone)]
pub struct SlotConfirmation {
    /// The slot number that progressed
    pub slot: u64,
    /// Raw commitment level: 0=Processed, 1=Confirmed, 2=Finalized
    pub commitment: u32,
}

pub type ConfirmationSender = mpsc::Sender<SlotConfirmation>;
pub type ConfirmationReceiver = mpsc::Receiver<SlotConfirmation>;

pub fn create_confirmation_channel(buffer: usize) -> (ConfirmationSender, ConfirmationReceiver) {
    mpsc::channel(buffer)
}

/// Carries tip median updates from L3 to L5
#[derive(Debug, Clone)]
pub struct TipUpdate {
    pub median_lamports: u64,
}

pub type TipSender = mpsc::Sender<TipUpdate>;
pub type TipReceiver = mpsc::Receiver<TipUpdate>;

pub fn create_tip_channel(buffer: usize) -> (TipSender, TipReceiver) {
    mpsc::channel(buffer)
}
