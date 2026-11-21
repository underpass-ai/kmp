//! How a live store's history stands against a committed bundle's.
//!
//! One question, answered from the events themselves and never from counts:
//! two histories with different lengths can still be branches, and a count
//! difference alone cannot say which side is ahead. Every caller that has to
//! decide what to do with a committed `.kmp/memory.jsonl` — open, the write
//! guard, `export`, `doctor` — asks this, so they cannot disagree.

use std::collections::{BTreeMap, HashMap};

use kmp_domain::{ContextUpdatedEvent, PortError};

use super::portability::parse_bundle;

/// The four answers. Counts are the authored events on each side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryRelation {
    /// The same events in the same order.
    Equal { events: u64 },
    /// The committed bundle is a strict prefix of the store: the store holds
    /// writes the bundle has not checkpointed yet.
    StoreAhead {
        store_events: u64,
        bundle_events: u64,
    },
    /// The store is a strict prefix of the bundle (an empty store included):
    /// a pull or a fresh clone brought events the store has not seen.
    BundleAhead {
        store_events: u64,
        bundle_events: u64,
    },
    /// Both sides hold an event the other does not, first at `position`
    /// (one-based). KMP will not invent an order for two branches.
    Diverged {
        position: u64,
        store_events: u64,
        bundle_events: u64,
    },
}

impl HistoryRelation {
    /// Compares two event streams position by position.
    pub fn classify(store: &[ContextUpdatedEvent], bundle: &[ContextUpdatedEvent]) -> Self {
        let store_events = store.len() as u64;
        let bundle_events = bundle.len() as u64;
        let shared = store.len().min(bundle.len());
        if let Some(position) = (0..shared).find(|position| store[*position] != bundle[*position]) {
            return Self::Diverged {
                position: position as u64 + 1,
                store_events,
                bundle_events,
            };
        }
        match store_events.cmp(&bundle_events) {
            std::cmp::Ordering::Equal => Self::Equal {
                events: store_events,
            },
            std::cmp::Ordering::Greater => Self::StoreAhead {
                store_events,
                bundle_events,
            },
            std::cmp::Ordering::Less => Self::BundleAhead {
                store_events,
                bundle_events,
            },
        }
    }

    /// Authored events the store holds.
    pub fn store_events(&self) -> u64 {
        match *self {
            Self::Equal { events } => events,
            Self::StoreAhead { store_events, .. }
            | Self::BundleAhead { store_events, .. }
            | Self::Diverged { store_events, .. } => store_events,
        }
    }

    /// Authored events the committed bundle holds.
    pub fn bundle_events(&self) -> u64 {
        match *self {
            Self::Equal { events } => events,
            Self::StoreAhead { bundle_events, .. }
            | Self::BundleAhead { bundle_events, .. }
            | Self::Diverged { bundle_events, .. } => bundle_events,
        }
    }

    /// Whether writing the store's history over the bundle keeps every
    /// event the bundle holds.
    pub fn bundle_is_prefix_of_store(&self) -> bool {
        matches!(self, Self::Equal { .. } | Self::StoreAhead { .. })
    }
}

/// Classifies two verified bundles: the store's authored export and the
/// committed file, both already projected through the same authored-memory
/// policy.
pub fn compare_bundle_histories(
    store_bundle: &str,
    committed_bundle: &str,
) -> Result<HistoryRelation, PortError> {
    let store = parse_bundle(store_bundle)?;
    let committed = parse_bundle(committed_bundle)?;
    Ok(HistoryRelation::classify(&store.events, &committed.events))
}

/// The committed events `store_bundle` does not hold, counted by about: what
/// writing it over `committed_bundle` would drop. Matched by the events
/// themselves, so a filtered bundle's local positions do not matter.
pub fn committed_events_not_kept(
    store_bundle: &str,
    committed_bundle: &str,
) -> Result<BTreeMap<String, u64>, PortError> {
    let store = parse_bundle(store_bundle)?;
    let committed = parse_bundle(committed_bundle)?;
    let mut held: HashMap<(&str, u64), Vec<&ContextUpdatedEvent>> = HashMap::new();
    for event in &store.events {
        held.entry((event.root_node_id.as_str(), event.revision))
            .or_default()
            .push(event);
    }
    let mut dropped = BTreeMap::new();
    for event in &committed.events {
        let kept = held
            .get(&(event.root_node_id.as_str(), event.revision))
            .is_some_and(|candidates| candidates.contains(&event));
        if !kept {
            *dropped.entry(event.root_node_id.clone()).or_insert(0) += 1;
        }
    }
    Ok(dropped)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn event(root: &str, revision: u64, hash: &str) -> ContextUpdatedEvent {
        ContextUpdatedEvent {
            root_node_id: root.to_string(),
            role: "agent".to_string(),
            revision,
            content_hash: hash.to_string(),
            changes: Vec::new(),
            idempotency_key: Some(format!("{root}:{revision}:{hash}")),
            logical_digest: None,
            requested_by: Some("history-test".to_string()),
            occurred_at: UNIX_EPOCH + Duration::from_secs(revision),
        }
    }

    #[test]
    fn each_side_ahead_is_told_apart_from_the_other() {
        let one = event("project:a", 1, "a1");
        let two = event("project:a", 2, "a2");
        let three = event("project:b", 1, "b1");
        let short = vec![one.clone(), two.clone()];
        let long = vec![one.clone(), two.clone(), three.clone()];

        assert_eq!(
            HistoryRelation::classify(&short, &long),
            HistoryRelation::BundleAhead {
                store_events: 2,
                bundle_events: 3
            }
        );
        assert_eq!(
            HistoryRelation::classify(&long, &short),
            HistoryRelation::StoreAhead {
                store_events: 3,
                bundle_events: 2
            }
        );
        assert_eq!(
            HistoryRelation::classify(&long, &long),
            HistoryRelation::Equal { events: 3 }
        );
        assert_eq!(
            HistoryRelation::classify(&[], &long),
            HistoryRelation::BundleAhead {
                store_events: 0,
                bundle_events: 3
            }
        );
    }

    #[test]
    fn what_a_narrower_bundle_would_drop_is_counted_by_about() {
        let bundle = |events: &[ContextUpdatedEvent]| {
            super::super::HeadStream::of(events)
                .expect("stream")
                .encode_as(None)
                .expect("bundle")
        };
        let a1 = event("project:a", 1, "a1");
        let b1 = event("project:b", 1, "b1");
        let a2 = event("project:a", 2, "a2");
        let b2 = event("project:b", 2, "b2");
        let committed = bundle(&[a1.clone(), b1.clone(), a2.clone(), b2.clone()]);

        let only_a = bundle(&[a1.clone(), a2.clone()]);
        let dropped = committed_events_not_kept(&only_a, &committed).expect("compare");
        assert_eq!(dropped, BTreeMap::from([("project:b".to_string(), 2)]));

        let short = bundle(&[a1.clone(), b1.clone()]);
        let dropped = committed_events_not_kept(&short, &committed).expect("compare");
        assert_eq!(
            dropped,
            BTreeMap::from([("project:a".to_string(), 1), ("project:b".to_string(), 1)])
        );

        let longer = bundle(&[a1, b1, a2, b2, event("project:c", 1, "c1")]);
        assert!(
            committed_events_not_kept(&longer, &committed)
                .expect("compare")
                .is_empty()
        );
    }

    #[test]
    fn the_same_events_in_another_order_are_a_divergence() {
        let a = event("project:a", 1, "a1");
        let b = event("project:b", 1, "b1");
        let c = event("project:a", 2, "a2");
        let relation =
            HistoryRelation::classify(&[b.clone(), a.clone()], &[a.clone(), b.clone(), c.clone()]);

        assert_eq!(
            relation,
            HistoryRelation::Diverged {
                position: 1,
                store_events: 2,
                bundle_events: 3
            }
        );
        assert!(!relation.bundle_is_prefix_of_store());
    }
}
