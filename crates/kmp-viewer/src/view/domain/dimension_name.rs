//! A lane the projection keeps.

/// The name of a memory dimension an intent asks to keep visible. Dimensions
/// are store-local vocabulary — unlike clocks and relation classes there is
/// no closed list, so the name stays opaque and the boundary reports the
/// ones the mounted store cannot resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DimensionName(String);

impl DimensionName {
    /// A dimension name exactly as requested.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The name as requested.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
