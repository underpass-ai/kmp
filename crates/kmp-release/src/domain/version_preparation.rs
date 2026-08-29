#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VersionPreparation {
    internal_dependencies: usize,
    mcpb_hash_reset: bool,
}

impl VersionPreparation {
    pub fn new(internal_dependencies: usize, mcpb_hash_reset: bool) -> Self {
        Self {
            internal_dependencies,
            mcpb_hash_reset,
        }
    }

    pub fn internal_dependencies(&self) -> usize {
        self.internal_dependencies
    }

    pub fn mcpb_hash_was_reset(&self) -> bool {
        self.mcpb_hash_reset
    }
}
