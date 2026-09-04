/// A relation one about declared between two of its own entries, as
/// `relate` reads it: enough to find the two facts and to carry the why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredEdge {
    pub from: String,
    pub to: String,
    pub rel: String,
    pub why: String,
    pub evidence: String,
}
