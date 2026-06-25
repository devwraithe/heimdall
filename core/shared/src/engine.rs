use crate::types::BundleOutcomeSummary;
use std::collections::VecDeque;
use tracing::info;

const MAX_RECENT_OUTCOMES: usize = 20;

pub struct OperationalState {
    pub current_slot: u64,
    pub latest_finalized_slot: u64,
    pub tip_median_lamports: u64,
    pub recent_outcomes: VecDeque<BundleOutcomeSummary>,
    pub active_bundle_count: u32,
    /// Retry metrics
    pub total_retries: u32,
    pub total_retries_succeeded: u32,
    pub total_retries_exhausted: u32,
    pub started_at: u64,
}

impl OperationalState {
    pub fn new() -> Self {
        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            current_slot: 0,
            latest_finalized_slot: 0,
            tip_median_lamports: 0,
            recent_outcomes: VecDeque::with_capacity(MAX_RECENT_OUTCOMES),
            active_bundle_count: 0,
            total_retries: 0,
            total_retries_succeeded: 0,
            total_retries_exhausted: 0,
            started_at,
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
            retry_attempt = outcome.retry_attempt,
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

    pub fn record_retry(&mut self) {
        self.total_retries += 1;
    }

    pub fn record_retry_success(&mut self) {
        self.total_retries_succeeded += 1;
    }

    pub fn record_retry_exhausted(&mut self) {
        self.total_retries_exhausted += 1;
    }

    pub fn uptime_seconds(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.started_at)
    }
}
