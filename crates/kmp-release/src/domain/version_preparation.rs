/// What one `version prepare` write set actually changed, so the operator can
/// see the surfaces that are easy to forget — the internal dependency pins, the
/// deliberately cleared MCPB digest and the Claude catalog ref that pins the
/// tag Claude Code clones.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionPreparation {
    internal_dependencies: usize,
    mcpb_hash_reset: bool,
    catalog_ref: String,
}

impl VersionPreparation {
    pub fn new(
        internal_dependencies: usize,
        mcpb_hash_reset: bool,
        catalog_ref: impl Into<String>,
    ) -> Self {
        Self {
            internal_dependencies,
            mcpb_hash_reset,
            catalog_ref: catalog_ref.into(),
        }
    }

    pub fn internal_dependencies(&self) -> usize {
        self.internal_dependencies
    }

    pub fn mcpb_hash_was_reset(&self) -> bool {
        self.mcpb_hash_reset
    }

    pub fn catalog_ref(&self) -> &str {
        &self.catalog_ref
    }
}
