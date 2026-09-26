use std::cell::Cell;
use std::collections::BTreeMap;

use crate::PortError;

use super::*;

/// Links held in memory, counting how many reads a walk makes.
#[derive(Default)]
struct Links {
    links: Vec<LifecycleLink>,
    reads: Cell<usize>,
    incomplete: Option<String>,
}

impl Links {
    fn with(
        mut self,
        newer: &str,
        older: &str,
        relation: LifecycleRelation,
        at: Option<&str>,
    ) -> Self {
        self.links.push(LifecycleLink {
            newer: newer.into(),
            older: older.into(),
            relation,
            occurred_at: at.map(str::to_string),
        });
        self
    }

    fn supersedes(self, newer: &str, older: &str) -> Self {
        self.with(newer, older, LifecycleRelation::Supersedes, None)
    }

    fn neighbours(&self, node: &str, pick: impl Fn(&LifecycleLink) -> &str) -> LifecycleNeighbours {
        self.reads.set(self.reads.get() + 1);
        LifecycleNeighbours {
            links: self
                .links
                .iter()
                .filter(|link| pick(link) == node)
                .cloned()
                .collect(),
            complete: self.incomplete.as_deref() != Some(node),
        }
    }
}

impl LifecycleLinkSource for Links {
    fn newer_than(&self, node: &str) -> Result<LifecycleNeighbours, PortError> {
        Ok(self.neighbours(node, |link| &link.older))
    }

    fn older_than(&self, node: &str) -> Result<LifecycleNeighbours, PortError> {
        Ok(self.neighbours(node, |link| &link.newer))
    }
}

fn nodes(steps: &[LifecycleStep]) -> Vec<&str> {
    steps.iter().map(|step| step.node.as_str()).collect()
}

#[test]
fn a_memory_nothing_replaced_is_its_own_head() {
    let chain = LifecycleChain::walk("a", &Links::default()).expect("walk");

    assert!(chain.is_empty());
    assert_eq!(chain.heads(), ["a"]);
    assert_eq!(chain.members(), ["a"]);
    assert!(!chain.cycle_cut() && !chain.truncated());
}

#[test]
fn every_lifecycle_relation_runs_from_the_newer_memory_to_the_older() {
    let links = Links::default()
        .supersedes("b", "a")
        .with("c", "b", LifecycleRelation::Corrects, None)
        .with("d", "c", LifecycleRelation::UpdatesState, None);

    let from_the_middle = LifecycleChain::walk("b", &links).expect("walk");

    assert_eq!(nodes(from_the_middle.newer()), ["c", "d"]);
    assert_eq!(nodes(from_the_middle.older()), ["a"]);
    assert_eq!(from_the_middle.heads(), ["d"]);
    assert_eq!(from_the_middle.members(), ["a", "b", "c", "d"]);
    let last = &from_the_middle.newer()[1];
    assert_eq!(
        (last.depth, last.from.as_str(), last.via),
        (2, "c", LifecycleRelation::UpdatesState)
    );
}

#[test]
fn successors_read_one_side_only() {
    let links = Links::default().supersedes("b", "a").supersedes("c", "b");

    let chain = LifecycleChain::successors("b", &links).expect("walk");

    assert_eq!(nodes(chain.newer()), ["c"]);
    assert!(chain.older().is_empty());
    assert_eq!(chain.heads(), ["c"]);
}

#[test]
fn a_fork_is_reported_in_occurred_then_id_order() {
    let links = Links::default()
        .with(
            "z-late",
            "a",
            LifecycleRelation::Supersedes,
            Some("2026-09-02T00:00:00Z"),
        )
        .with(
            "b-early",
            "a",
            LifecycleRelation::Supersedes,
            Some("2026-09-01T00:00:00Z"),
        )
        .with("m-undated", "a", LifecycleRelation::Corrects, None);

    let chain = LifecycleChain::walk("a", &links).expect("walk");

    let fork = &chain.forks()[0];
    assert_eq!((fork.at.as_str(), fork.side), ("a", LifecycleSide::Newer));
    assert_eq!(fork.members, ["m-undated", "b-early", "z-late"]);
    assert_eq!(chain.heads(), ["m-undated", "b-early", "z-late"]);
}

#[test]
fn a_cycle_is_cut_and_has_no_head() {
    let links = Links::default()
        .supersedes("b", "a")
        .supersedes("c", "b")
        .supersedes("a", "c");

    let chain = LifecycleChain::successors("a", &links).expect("walk");

    assert!(chain.cycle_cut());
    assert_eq!(nodes(chain.newer()), ["b", "c"]);
    assert!(chain.heads().is_empty());
}

#[test]
fn two_routes_to_one_memory_are_a_merge_and_not_a_cycle() {
    let links = Links::default()
        .supersedes("b", "a")
        .supersedes("c", "a")
        .supersedes("d", "b")
        .supersedes("d", "c");

    let chain = LifecycleChain::successors("a", &links).expect("walk");

    assert!(!chain.cycle_cut());
    assert_eq!(nodes(chain.newer()), ["b", "c", "d"]);
    assert_eq!(chain.heads(), ["d"]);
}

#[test]
fn the_walk_stops_at_its_depth_bound_and_says_so() {
    let mut links = Links::default();
    for index in 0..40 {
        links = links.supersedes(&format!("v{}", index + 1), &format!("v{index}"));
    }

    let chain = LifecycleChain::successors("v0", &links).expect("walk");

    assert_eq!(chain.newer().len(), MAX_LIFECYCLE_DEPTH);
    assert!(chain.truncated());
    assert!(
        chain.heads().is_empty(),
        "v32 was replaced; it is not a head"
    );
    // One read per hop, and one to learn the edge of the bound was not the end.
    assert_eq!(links.reads.get(), MAX_LIFECYCLE_DEPTH + 1);
}

#[test]
fn a_chain_exactly_as_deep_as_the_bound_is_complete() {
    let mut links = Links::default();
    for index in 0..MAX_LIFECYCLE_DEPTH {
        links = links.supersedes(&format!("v{}", index + 1), &format!("v{index}"));
    }

    let chain = LifecycleChain::successors("v0", &links).expect("walk");

    assert!(!chain.truncated());
    assert_eq!(chain.heads(), [format!("v{MAX_LIFECYCLE_DEPTH}")]);
}

#[test]
fn a_source_that_stopped_short_truncates_the_walk() {
    let links = Links {
        incomplete: Some("a".into()),
        ..Links::default()
    }
    .supersedes("b", "a");

    let chain = LifecycleChain::successors("a", &links).expect("walk");

    assert!(chain.truncated());
    assert_eq!(chain.heads(), ["b"]);
}

#[test]
fn the_walk_reaches_no_more_than_its_member_bound() {
    let mut links = Links::default();
    for index in 0..(MAX_LIFECYCLE_MEMBERS + 10) {
        links = links.supersedes(&format!("n{index:04}"), "root");
    }

    let chain = LifecycleChain::successors("root", &links).expect("walk");

    assert_eq!(chain.newer().len(), MAX_LIFECYCLE_MEMBERS);
    assert!(chain.truncated());
}

#[test]
fn two_links_between_the_same_memories_count_once() {
    let links =
        Links::default()
            .supersedes("b", "a")
            .with("b", "a", LifecycleRelation::Corrects, None);

    let chain = LifecycleChain::walk("a", &links).expect("walk");

    assert!(chain.forks().is_empty());
    assert_eq!(chain.newer()[0].via, LifecycleRelation::Supersedes);
}

#[test]
fn the_relation_names_parse_back() {
    let names = LifecycleRelation::ALL
        .iter()
        .map(|relation| {
            (
                relation.as_str(),
                LifecycleRelation::parse(relation.as_str()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(names.len(), 3);
    assert!(names.values().all(Option::is_some));
    assert_eq!(LifecycleRelation::parse("contradicts"), None);
}
