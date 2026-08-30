/// How a store can be reached, which is what decides whether anyone will ever
/// find it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreReach {
    /// The per-user default: `kmp-mcp` opens it from anywhere with nothing set.
    User,
    /// A project store, reachable from inside its own repository.
    Project,
    /// No rule resolves to it. `KMP_MCP_DATA_DIR` by hand is the only way in.
    Unreachable,
}

impl StoreReach {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Unreachable => "unreachable",
        }
    }
}
