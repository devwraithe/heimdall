use crate::types::LifecycleEntry;
use std::fs::OpenOptions;
use std::io::Write;
use tracing::error;

fn log_path() -> String {
    std::env::var("LIFECYCLE_LOG_PATH").unwrap_or_else(|_| "lifecycle.log".to_string())
}

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

    let path = log_path();
    let mut file = match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            error!(path = %path, error = %e, "Failed to open lifecycle log file");
            return;
        }
    };

    if let Err(e) = writeln!(file, "{}", json) {
        error!(path = %path, error = %e, "Failed to write lifecycle entry");
    }
}
