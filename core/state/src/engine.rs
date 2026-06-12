use crate::heimdall::{BundleOutcomeSummary, OperationalSnapshot};
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

/// Maximum number of recent outcomes to retain
const MAX_RECENT_OUTCOMES: usize = 20;

/// L5's in-memory operational state.
/// Aggregates data from L1-L4 and produces
/// OperationalSnapshot for L6.
pub struct OperationalState {
    /// Current slot from L1 slot stream
    pub current_slot: u64,
    /// Latest finalized slot from L1
    pub latest_finalized_slot: u64,
    /// Latest tip median from L3
    pub tip_median_lamports: u64,
    /// Rolling window of recent bundle outcomes
    pub recent_outcomes: VecDeque<BundleOutcomeSummary>,
    /// Number of bundles currently in flight
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

    /// Updates current slot from L1
    pub fn update_slot(&mut self, slot: u64, is_finalized: bool) {
        self.current_slot = self.current_slot.max(slot);
        if is_finalized {
            self.latest_finalized_slot = self.latest_finalized_slot.max(slot);
        }
    }

    /// Updates tip median from L3
    pub fn update_tip(&mut self, median_lamports: u64) {
        self.tip_median_lamports = median_lamports;
    }

    /// Records a bundle outcome from L4
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

    /// Increments active bundle count when L3 submits
    pub fn bundle_submitted(&mut self) {
        self.active_bundle_count += 1;
    }

    /// Decrements active bundle count when L4 evicts
    pub fn bundle_resolved(&mut self) {
        self.active_bundle_count = self.active_bundle_count.saturating_sub(1);
    }

    /// Produces a snapshot for L6 consumption
    pub fn snapshot(&self) -> OperationalSnapshot {
        let snapshot_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        OperationalSnapshot {
            current_slot: self.current_slot,
            latest_finalized_slot: self.latest_finalized_slot,
            tip_median_lamports: self.tip_median_lamports,
            recent_outcomes: self.recent_outcomes.iter().cloned().collect(),
            active_bundle_count: self.active_bundle_count,
            snapshot_at,
        }
    }
}
