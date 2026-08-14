use std::time::{SystemTime, UNIX_EPOCH};

const DEBUG_FLAG_ENV: &str = "AGENTIC_DEBUG";
const LEGACY_DEBUG_FLAG_ENV: &str = "AGENTIC_E2E_DEBUG";

pub(crate) fn debug_log(message: impl AsRef<str>) {
    if !debug_enabled() {
        return;
    }

    eprintln!("{}", format_log_line(message.as_ref()));
}

pub(crate) fn debug_log_value(label: &str, value: impl std::fmt::Display) {
    debug_log(format!("{label}: {value}"));
}

fn debug_enabled() -> bool {
    [DEBUG_FLAG_ENV, LEGACY_DEBUG_FLAG_ENV]
        .into_iter()
        .any(|name| {
            std::env::var(name)
                .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
                .unwrap_or(false)
        })
}

fn format_log_line(message: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("[agentic-debug][{millis}] {message}")
}
