use super::LifecycleRelation;

/// A memory a lifecycle walk reached, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleStep {
    pub node: String,
    /// Hops from the memory the walk started at, from 1.
    pub depth: usize,
    /// The memory one hop nearer the start that reached this one.
    pub from: String,
    pub via: LifecycleRelation,
    pub occurred_at: Option<String>,
}
