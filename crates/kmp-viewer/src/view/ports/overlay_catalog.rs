//! Which telemetry series this process can actually draw.

use crate::view::domain::OverlayName;

/// The exact telemetry vocabulary mounted beside the loom. The MCP face uses
/// it to distinguish a requested overlay from a name the actual viewer
/// reader cannot resolve — reported as unhonored, never drawn as if it were
/// data.
pub trait OverlayCatalog: Send + Sync {
    /// Publishes the mounted vocabulary, replacing what was known before.
    fn publish(&self, names: Vec<OverlayName>);

    /// Whether the mounted reader resolves this series.
    fn contains(&self, name: &OverlayName) -> bool;
}
