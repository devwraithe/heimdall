use crate::types::BundleOutcomeSummary;
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

const MAX_RECENT_OUTCOMES: usize = 20;

pub struct OperationalState {
    pub current_slot: u64,
    pub latest_finalized_slot: u64,
    pub tip_median_lamports: u64,
    pub recent_outcomes: VecDeque<BundleOutcomeSummary>,
    pub active_bundle_count: u32,
}

impl OperationalState {
    pub fn new() -> Self {
        Self {
            current_slot: 0,
            latest_finalized_slot: 0,
            tip_median_lamports: 0,
            recent_outcomes: VecDeque::with_capacity(MAX_RECENT_OUTCOMES),
            active_bundle_count: 0,
        }
    }

    pub fn update_slot(&mut self, slot: u64, is_finalized: bool) {
        self.current_slot = self.current_slot.max(slot);
        if is_finalized {
            self.latest_finalized_slot = self.latest_finalized_slot.max(slot);
        }
    }

    pub fn update_tip(&mut self, median_lamports: u64) {
        self.tip_median_lamports = median_lamports;
    }

    pub fn record_outcome(&mut self, outcome: BundleOutcomeSummary) {
        info!(
            bundle_id = %outcome.bundle_id,
            stage = %outcome.stage,
            "Operational state recording outcome"
        );
        if self.recent_outcomes.len() >= MAX_RECENT_OUTCOMES {
            self.recent_outcomes.pop_front();
        }
        self.recent_outcomes.push_back(outcome);
    }

    pub fn bundle_submitted(&mut self) {
        self.active_bundle_count += 1;
    }

    pub fn bundle_resolved(&mut self) {
        self.active_bundle_count = self.active_bundle_count.saturating_sub(1);
    }
}
