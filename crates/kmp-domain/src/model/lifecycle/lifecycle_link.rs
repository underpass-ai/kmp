use super::LifecycleRelation;

/// One declared lifecycle relation: `newer` takes over from `older`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleLink {
    pub newer: String,
    pub older: String,
    pub relation: LifecycleRelation,
    /// When the relation says the takeover happened, as stored; absent is a
    /// silence, not a claim that it came first.
    pub occurred_at: Option<String>,
}
