use crate::types::LifecycleEntry;
use std::fs::OpenOptions;
use std::io::Write;
use tracing::error;

/// Path to the lifecycle log file
const LOG_PATH: &str = "lifecycle.log";

/// Appends a single lifecycle entry as a JSON line to the log file.
/// Each line is a complete, self-contained JSON object.
pub fn write_entry(entry: &LifecycleEntry) {
    let json = match serde_json::to_string(entry) {
        Ok(j) => j,
        Err(e) => {
            error!(error = %e, "Failed to serialize lifecycle entry");
            return;
        }
    };

    let mut file = match OpenOptions::new().create(true).append(true).open(LOG_PATH) {
        Ok(f) => f,
        Err(e) => {
            error!(error = %e, "Failed to open lifecycle log file");
            return;
        }
    };

    if let Err(e) = writeln!(file, "{}", json) {
        error!(error = %e, "Failed to write lifecycle entry");
    }
}
