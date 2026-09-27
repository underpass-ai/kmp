/// A proposed search expansion that is not kept, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedSearchExpansion {
    pub entry_ref: String,
    pub expansion: String,
    pub why: String,
}
