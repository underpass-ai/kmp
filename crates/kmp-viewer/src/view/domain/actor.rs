//! Who has a hand on the loom.

/// The named mover of a view change. `human` is the person at the loom; any
/// other name is an agent, shown to the human so a view can never be
/// rearranged anonymously.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Actor(String);

impl Actor {
    /// The person at the loom.
    pub fn human() -> Self {
        Self("human".to_string())
    }

    /// An actor as a caller named it — an agent id, or `human`.
    pub fn named(name: &str) -> Self {
        Self(name.to_string())
    }

    /// Whether this is the person rather than an agent.
    pub fn is_human(&self) -> bool {
        self.0 == "human"
    }

    /// The name as given.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
