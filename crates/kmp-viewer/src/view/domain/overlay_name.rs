//! A telemetry series aligned over the loom.

/// The name of an observability series an intent asks to draw on the shared
/// temporal axis. Series names are process-local vocabulary published by the
/// mounted telemetry reader; a name that catalog does not hold is reported
/// as unhonored, never drawn as if it were data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayName(String);

impl OverlayName {
    /// A series name exactly as requested.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The name as requested.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
