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
