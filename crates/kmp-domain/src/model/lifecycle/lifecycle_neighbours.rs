use super::LifecycleLink;

/// The lifecycle links of one memory on one side, as a source read them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LifecycleNeighbours {
    pub links: Vec<LifecycleLink>,
    /// False when the source stopped at its per-node bound and more links
    /// may exist: the walk then reports itself truncated.
    pub complete: bool,
}

impl LifecycleNeighbours {
    pub fn complete(links: Vec<LifecycleLink>) -> Self {
        Self {
            links,
            complete: true,
        }
    }
}
