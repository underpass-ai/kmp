use std::time::SystemTime;

/// Returns `true` when the `AGENTIC_DEBUG` environment variable is set.
pub fn debug_enabled() -> bool {
    std::env::var("AGENTIC_DEBUG").is_ok()
}

/// Logs a message to stderr when `AGENTIC_DEBUG` is set.
pub fn debug_log(message: &str) {
    if debug_enabled() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        eprintln!("[AGENTIC {now}] {message}");
    }
}

/// Logs a key-value pair to stderr when `AGENTIC_DEBUG` is set.
pub fn debug_log_value(key: &str, value: impl std::fmt::Display) {
    if debug_enabled() {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        eprintln!("[AGENTIC {now}] {key}: {value}");
    }
}
