use std::collections::{BTreeMap, BTreeSet};

use crate::{PortError, temporal_instant_nanos};

use super::{
    LifecycleFork, LifecycleLink, LifecycleLinkSource, LifecycleNeighbours, LifecycleSide,
    LifecycleStep, MAX_LIFECYCLE_DEPTH, MAX_LIFECYCLE_MEMBERS,
};

/// One side of a walk, breadth first.
pub(super) struct SideWalk {
    pub(super) steps: Vec<LifecycleStep>,
    pub(super) terminal: Vec<String>,
    pub(super) forks: Vec<LifecycleFork>,
    pub(super) cycle_cut: bool,
    pub(super) truncated: bool,
}

impl SideWalk {
    pub(super) fn run<S: LifecycleLinkSource + ?Sized>(
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
) -> Result<LifecycleNeighbours, PortError> {
    match side {
        LifecycleSide::Newer => source.newer_than(node),
        LifecycleSide::Older => source.older_than(node),
    }
}

/// The links in `(occurred, id)` order, one per neighbour: the first of
/// several links between the same two memories speaks for them.
fn ordered(
    side: LifecycleSide,
    neighbours: LifecycleNeighbours,
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
