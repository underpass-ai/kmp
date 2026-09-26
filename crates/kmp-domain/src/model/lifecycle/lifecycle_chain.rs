use crate::PortError;

use super::side_walk::SideWalk;
use super::{LifecycleFork, LifecycleLinkSource, LifecycleSide, LifecycleStep};

/// How many hops a walk goes on each side before it stops and says so.
pub const MAX_LIFECYCLE_DEPTH: usize = 32;
/// How many memories a walk reaches on each side before it stops and says so.
pub const MAX_LIFECYCLE_MEMBERS: usize = 256;
/// How many links of one type a source reads for one memory.
pub const MAX_LIFECYCLE_LINKS_PER_NODE: u32 = 16;

/// The declared lifecycle around one memory.
///
/// `newer` and `older` are breadth first, nearest first; inside one hop the
/// links of a memory are read in `(occurred, id)` order, an absent clock
/// first. A memory is visited once: a link back to one already reached is
/// not walked again, and when that memory is the start or lies on the path
/// that reached the link, the walk says it cut a cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleChain {
    from: String,
    newer: Vec<LifecycleStep>,
    older: Vec<LifecycleStep>,
    heads: Vec<String>,
    forks: Vec<LifecycleFork>,
    cycle_cut: bool,
    truncated: bool,
}

impl LifecycleChain {
    /// Walks both sides of `from`'s lifecycle.
    pub fn walk<S: LifecycleLinkSource + ?Sized>(
        from: &str,
        source: &S,
    ) -> Result<Self, PortError> {
        let newer = SideWalk::run(from, source, LifecycleSide::Newer)?;
        let older = SideWalk::run(from, source, LifecycleSide::Older)?;
        let mut forks = newer.forks;
        forks.extend(older.forks);
        Ok(Self {
            from: from.to_string(),
            heads: newer.terminal,
            newer: newer.steps,
            older: older.steps,
            forks,
            cycle_cut: newer.cycle_cut || older.cycle_cut,
            truncated: newer.truncated || older.truncated,
        })
    }

    /// Walks only toward what replaced `from`: the heads without the history.
    pub fn successors<S: LifecycleLinkSource + ?Sized>(
        from: &str,
        source: &S,
    ) -> Result<Self, PortError> {
        let newer = SideWalk::run(from, source, LifecycleSide::Newer)?;
        Ok(Self {
            from: from.to_string(),
            heads: newer.terminal,
            newer: newer.steps,
            older: Vec::new(),
            forks: newer.forks,
            cycle_cut: newer.cycle_cut,
            truncated: newer.truncated,
        })
    }

    pub fn from(&self) -> &str {
        &self.from
    }

    /// What took over from `from`, transitively, nearest first.
    pub fn newer(&self) -> &[LifecycleStep] {
        &self.newer
    }

    /// What `from` took over from, transitively, nearest first.
    pub fn older(&self) -> &[LifecycleStep] {
        &self.older
    }

    /// The memories nothing replaced, among `from` and what replaced it,
    /// ordered by `(occurred, id)` of the link that reached them. `from` is
    /// its own head when nothing replaced it. A memory the walk could not
    /// finish reading is not a head, and a cycle has none.
    pub fn heads(&self) -> &[String] {
        &self.heads
    }

    /// Where the succession branches or merges.
    pub fn forks(&self) -> &[LifecycleFork] {
        &self.forks
    }

    /// Whether a link led back onto the path that reached it.
    pub fn cycle_cut(&self) -> bool {
        self.cycle_cut
    }

    /// Whether a bound stopped the walk before it ran out of links.
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Whether any link touches `from` at all.
    pub fn is_empty(&self) -> bool {
        self.newer.is_empty() && self.older.is_empty()
    }

    /// Every member, oldest side first: the older steps from the furthest
    /// in, `from`, then the newer steps nearest first.
    pub fn members(&self) -> Vec<&str> {
        self.older
            .iter()
            .rev()
            .map(|step| step.node.as_str())
            .chain(std::iter::once(self.from.as_str()))
            .chain(self.newer.iter().map(|step| step.node.as_str()))
            .collect()
    }
}
