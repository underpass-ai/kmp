/// Which way along a lifecycle a walk goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LifecycleSide {
    /// Toward what replaced a memory: its successors, up to the heads.
    Newer,
    /// Toward what a memory replaced: its predecessors.
    Older,
}
