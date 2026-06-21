use crate::heimdall::{BundleOutcomeSummary as ProtoBundleOutcomeSummary, OperationalSnapshot};
use shared::engine::OperationalState;
use std::time::{SystemTime, UNIX_EPOCH};

/// Converts OperationalState into a gRPC OperationalSnapshot
pub fn to_snapshot(state: &OperationalState) -> OperationalSnapshot {
    let snapshot_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    OperationalSnapshot {
        current_slot: state.current_slot,
        latest_finalized_slot: state.latest_finalized_slot,
        tip_median_lamports: state.tip_median_lamports,
        recent_outcomes: state
            .recent_outcomes
            .iter()
            .map(|o| ProtoBundleOutcomeSummary {
                bundle_id: o.bundle_id.clone(),
                slot: o.slot,
                stage: o.stage.clone(),
                failure_reason: o.failure_reason.clone(),
                tip_lamports: o.tip_lamports,
                blockhash: o.blockhash.clone(),
                submitted_at: o.submitted_at,
            })
            .collect(),
        active_bundle_count: state.active_bundle_count,
        snapshot_at,
    }
}
