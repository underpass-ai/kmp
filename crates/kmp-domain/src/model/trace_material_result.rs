/// Selection over discovered routes; completeness is relative to declared groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceMaterialResult {
    pub selected_candidates: Vec<u32>,
    pub material_refs: Vec<String>,
    pub covered_groups: Vec<u32>,
    pub incomplete_groups: Vec<u32>,
    pub benefit: u32,
    pub evaluated: u32,
    pub pruned_by_width: u32,
}
