use tokio::sync::mpsc;

// Commitment status mirroring Yellowstone's three levels
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

// Observable network events emitted by L1
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    // Slot update with its commitment status
    SlotUpdate { slot: u64, status: SlotStatus },
    // Upcoming leader window for the next N slots
    LeaderWindow { slot: u64, leader: String },
}

pub type EventSender = mpsc::Sender<NetworkEvent>;
pub type EventReceiver = mpsc::Receiver<NetworkEvent>;

// Creates the event channel for L1 and L2 communication
pub fn create_event_channel(buffer: usize) -> (EventSender, EventReceiver) {
    mpsc::channel(buffer)
}

// Observation
pub enum CongestionLevel {
    High,
    Medium,
    Low,
}
pub struct Observation {
    pub current_slot: u64,
    pub current_leader: String,
    pub target_leader: String,
    pub target_leader_start_slot: u64,
    pub target_leader_end_slot: u64,
    pub slots_until_target_leader: u64,
    pub median_tip_lamports: u64,
    pub p90_tip_lamports: u64,
    pub tps: u64,
    pub congestion_level: CongestionLevel,
    pub timestamp: Option<u64>,
}
