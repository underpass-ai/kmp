/// How a store can be reached, which is what decides whether anyone will ever
/// find it again.
///
/// The first six name the resolver's own rules (`ResolvedDataDir::rule_name`)
/// and use its words, so `info`, `config` and `memories` cannot describe the
/// same store two ways. The label is decided by the real resolution, never
/// guessed from where a path happens to live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreReach {
    /// `KMP_MCP_DATA_DIR` names it for this process.
    Env,
    /// A project store, reachable from inside its own directory.
    Project,
    /// The main checkout's store, reached from one of its git worktrees.
    Worktree,
    /// The selection saved with `kmp-mcp config memory-store`.
    Saved,
    /// The per-user default: `kmp-mcp` opens it from anywhere with nothing set.
    User,
    /// The per-user default, opened because a project store beside a
    /// committed bundle could not be.
    UserFallback,
    /// No rule resolves to it. `KMP_MCP_DATA_DIR` by hand is the only way in.
    Unreachable,
}

impl StoreReach {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Project => "project",
            Self::Worktree => "worktree",
            Self::Saved => "saved",
            Self::User => "user",
            Self::UserFallback => "user fallback",
            Self::Unreachable => "unreachable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StoreReach;

    #[test]
    fn every_reach_uses_the_resolvers_own_rule_name() {
        let labels = [
            (StoreReach::Env, "env"),
            (StoreReach::Project, "project"),
            (StoreReach::Worktree, "worktree"),
            (StoreReach::Saved, "saved"),
            (StoreReach::User, "user"),
            (StoreReach::UserFallback, "user fallback"),
            (StoreReach::Unreachable, "unreachable"),
        ];
        for (reach, label) in labels {
            assert_eq!(reach.as_str(), label);
        }
    }
}
