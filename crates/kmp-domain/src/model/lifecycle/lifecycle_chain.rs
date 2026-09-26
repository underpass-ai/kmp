use std::collections::{BTreeMap, BTreeSet};

use crate::{PortError, temporal_instant_nanos};

use super::{LifecycleFork, LifecycleLink, LifecycleLinkSource, LifecycleSide, LifecycleStep};

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

/// One side of a walk, breadth first.
struct SideWalk {
    steps: Vec<LifecycleStep>,
    terminal: Vec<String>,
    forks: Vec<LifecycleFork>,
    cycle_cut: bool,
    truncated: bool,
}

impl SideWalk {
    fn run<S: LifecycleLinkSource + ?Sized>(
        from: &str,
        source: &S,
        side: LifecycleSide,
    ) -> Result<Self, PortError> {
        let mut walk = Self {
            steps: Vec::new(),
            terminal: Vec::new(),
            forks: Vec::new(),
            cycle_cut: false,
            truncated: false,
        };
        let mut visited = BTreeSet::from([from.to_string()]);
        let mut parent = BTreeMap::<String, String>::new();
        let mut frontier = vec![from.to_string()];
        for depth in 1..=MAX_LIFECYCLE_DEPTH {
            let mut next = Vec::new();
            for node in &frontier {
                let links = ordered(side, read(source, node, side)?, &mut walk.truncated);
                if links.is_empty() {
                    walk.terminal.push(node.clone());
                    continue;
                }
                if links.len() > 1 {
                    walk.forks.push(LifecycleFork {
                        at: node.clone(),
                        side,
                        members: links
                            .iter()
                            .map(|link| other(link, side).to_string())
                            .collect(),
                    });
                }
                for link in links {
                    let reached = other(&link, side).to_string();
                    if visited.contains(&reached) {
                        if reached == from || lies_on_path(&reached, node, &parent) {
                            walk.cycle_cut = true;
                        }
                        continue;
                    }
                    if walk.steps.len() == MAX_LIFECYCLE_MEMBERS {
                        walk.truncated = true;
                        continue;
                    }
                    visited.insert(reached.clone());
                    parent.insert(reached.clone(), node.clone());
                    walk.steps.push(LifecycleStep {
                        node: reached.clone(),
                        depth,
                        from: node.clone(),
                        via: link.relation,
                        occurred_at: link.occurred_at,
                    });
                    next.push(reached);
                }
            }
            frontier = next;
            if frontier.is_empty() {
                return Ok(walk.with_ordered_terminal());
            }
        }
        // The depth bound left a frontier unread: a memory at its edge is a
        // head only if nothing replaced it, and the walk is truncated only
        // if something did.
        for node in &frontier {
            let links = read(source, node, side)?;
            if links.links.is_empty() && links.complete {
                walk.terminal.push(node.clone());
            } else {
                walk.truncated = true;
            }
        }
        Ok(walk.with_ordered_terminal())
    }

    /// The terminal memories in `(occurred, id)` order of the link that
    /// reached them; the start, reached by none, first.
    fn with_ordered_terminal(mut self) -> Self {
        let reached_at = self
            .steps
            .iter()
            .map(|step| (step.node.as_str(), step.occurred_at.as_deref()))
            .collect::<BTreeMap<_, _>>();
        let key = |node: &String| {
            let occurred = reached_at.get(node.as_str()).copied().flatten();
            (
                reached_at.contains_key(node.as_str()),
                occurred.and_then(temporal_instant_nanos),
                occurred.unwrap_or_default().to_string(),
                node.clone(),
            )
        };
        self.terminal.sort_by_key(key);
        self
    }
}

fn read<S: LifecycleLinkSource + ?Sized>(
    source: &S,
    node: &str,
    side: LifecycleSide,
) -> Result<super::LifecycleNeighbours, PortError> {
    match side {
        LifecycleSide::Newer => source.newer_than(node),
        LifecycleSide::Older => source.older_than(node),
    }
}

/// The links in `(occurred, id)` order, one per neighbour: the first of
/// several links between the same two memories speaks for them.
fn ordered(
    side: LifecycleSide,
    neighbours: super::LifecycleNeighbours,
    truncated: &mut bool,
) -> Vec<LifecycleLink> {
    if !neighbours.complete {
        *truncated = true;
    }
    let mut links = neighbours.links;
    links.sort_by(|left, right| {
        order_key(left, side)
            .cmp(&order_key(right, side))
            .then_with(|| left.relation.cmp(&right.relation))
    });
    let mut seen = BTreeSet::new();
    links.retain(|link| seen.insert(other(link, side).to_string()));
    links
}

fn order_key(link: &LifecycleLink, side: LifecycleSide) -> (Option<i128>, &str, &str) {
    let occurred = link.occurred_at.as_deref();
    (
        occurred.and_then(temporal_instant_nanos),
        occurred.unwrap_or_default(),
        other(link, side),
    )
}

fn other(link: &LifecycleLink, side: LifecycleSide) -> &str {
    match side {
        LifecycleSide::Newer => &link.newer,
        LifecycleSide::Older => &link.older,
    }
}

/// Whether `candidate` is `node` or one of the memories that reached it.
fn lies_on_path(candidate: &str, node: &str, parent: &BTreeMap<String, String>) -> bool {
    let mut current = node;
    loop {
        if current == candidate {
            return true;
        }
        match parent.get(current) {
            Some(up) => current = up,
            None => return false,
        }
    }
}
