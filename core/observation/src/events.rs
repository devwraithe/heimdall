use tokio::sync::mpsc;

/// Commitment status mirroring Yellowstone's three levels
#[derive(Debug, Clone)]
pub enum SlotStatus {
    Processed,
    Confirmed,
    Finalized,
    Unknown,
}

impl SlotStatus {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => SlotStatus::Processed,
            1 => SlotStatus::Confirmed,
            2 => SlotStatus::Finalized,
            _ => SlotStatus::Unknown,
        }
    }
}

/// All observable network events emitted by L1
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// A slot update with its commitment status
    SlotUpdate { slot: u64, status: SlotStatus },
    /// Upcoming leader window for the next N slots
    LeaderWindow { slot: u64, leader: String },
    /// Stub — block data observed (not yet implemented)
    BlockObserved { slot: u64 },
    /// Stub — transaction observed (not yet implemented)
    TransactionObserved { signature: String },
}

/// Type alias for the event sender
pub type EventSender = mpsc::Sender<NetworkEvent>;

/// Type alias for the event receiver
pub type EventReceiver = mpsc::Receiver<NetworkEvent>;

/// Creates the event channel for L1 → L2 communication
pub fn create_event_channel(buffer: usize) -> (EventSender, EventReceiver) {
    mpsc::channel(buffer)
}
