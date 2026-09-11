use super::trace_material_set::TraceMaterialSet;

pub(super) struct TraceMaterialState {
    pub indexes: Vec<usize>,
    pub material: TraceMaterialSet,
    pub benefit: u32,
    pub priority: u64,
}

impl TraceMaterialState {
    /// Actual benefit first; lower material, fewer routes and lexical indexes break ties.
    pub fn actual_order(&self, other: &Self) -> std::cmp::Ordering {
        other
            .benefit
            .cmp(&self.benefit)
            .then_with(|| self.material.count().cmp(&other.material.count()))
            .then_with(|| self.indexes.len().cmp(&other.indexes.len()))
            .then_with(|| self.indexes.cmp(&other.indexes))
    }
}
