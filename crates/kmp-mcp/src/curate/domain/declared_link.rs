/// A relation a writer declared between two facts, with its stated reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeclaredLink {
    pub from: String,
    pub to: String,
    pub rel: String,
    pub why: String,
    pub evidence: String,
}
