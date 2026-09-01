//! What a focus, selection or trace end points at.

/// A memory ref exactly as a caller named it. Refs are opaque identifiers:
/// the view stores them so the browser and the agent can read back what was
/// asked for; whether one exists in the store is the boundary's question,
/// answered before an intent reaches this aggregate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRef(String);

impl MemoryRef {
    /// A ref, byte for byte as given.
    pub fn new(reference: impl Into<String>) -> Self {
        Self(reference.into())
    }

    /// The ref, byte for byte.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
