use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::CommitmentConfig;
use std::collections::HashMap;
use tracing::info;

pub struct LeaderSchedule {
    /// Maps slot number to validator identity
    schedule: HashMap<u64, String>,
}

impl LeaderSchedule {
    pub fn fetch(rpc_url: &str, current_slot: u64) -> Result<Self> {
        let client =
            RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

        info!(current_slot, "Fetching leader schedule");

        let schedule_map = client
            .get_leader_schedule(Some(current_slot))?
            .ok_or_else(|| anyhow::anyhow!("Leader schedule not available"))?;

        // Calculate epoch start slot
        let slots_per_epoch = 432_000u64;
        let epoch_start_slot = (current_slot / slots_per_epoch) * slots_per_epoch;

        // Reverse map: slot_index and validator identity
        let mut schedule = HashMap::new();
        for (validator, slot_indices) in schedule_map {
            for slot_index in slot_indices {
                let absolute_slot = epoch_start_slot + slot_index as u64;
                schedule.insert(absolute_slot, validator.clone());
            }
        }

        info!(
            total_slots = schedule.len(),
            epoch_start_slot, "Leader schedule loaded"
        );

        Ok(Self { schedule })
    }

    /// Returns the leader for a specific slot
    pub fn _get_leader(&self, slot: u64) -> Option<&str> {
        self.schedule.get(&slot).map(|s| s.as_str())
    }

    /// Returns leaders for the next N slots from current
    pub fn get_upcoming_leaders(&self, current_slot: u64, window: u64) -> Vec<(u64, &str)> {
        (current_slot..current_slot + window)
            .filter_map(|slot| {
                self.schedule
                    .get(&slot)
                    .map(|leader| (slot, leader.as_str()))
            })
            .collect()
    }
}
