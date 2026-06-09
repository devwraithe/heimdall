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
