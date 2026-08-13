use kmp_domain::{KmpMode, ResolutionTier};

/// Hint about which RPC endpoint triggered this render. Influences the mode
/// heuristic thresholds — scoped paths tolerate more pruning, sessions need
/// richer context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndpointHint {
    /// `GetContext` — broad neighborhood retrieval.
    #[default]
    Neighborhood,
    /// `GetContextPath` — specific path to a target node.
    FocusedPath,
    /// `RehydrateSession` — multi-role snapshot.
    SessionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextRenderOptions {
    pub focus_node_id: Option<String>,
    pub token_budget: Option<u32>,
    /// Maximum tier to render. `None` means all tiers (backward compatible).
    pub max_tier: Option<ResolutionTier>,
    /// Rehydration mode. `Auto` lets the kernel choose based on token pressure.
    pub rehydration_mode: KmpMode,
    /// Which endpoint is requesting this render.
    pub endpoint_hint: EndpointHint,
}
