use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::KmpBundle;
use kmp_proto::v1beta1::ExpiredMemory;
use prost_types::Timestamp;

use super::scalars::timestamp_from_sort_or_rfc3339;

const SECONDS_PER_DAY: i64 = 86_400;

/// Which memories in a bundle have stopped standing, and when.
///
/// KMP models three lifecycles and its own guide insists they are different:
/// `supersedes` replaces while preserving history, `contradicts` holds two
/// live claims in tension, and `valid_until` ends applicability without
/// naming a successor. Wake and Ask each read one of the three. The proof
/// they return declares an `expired` list that nothing ever filled, so an
/// entry whose applicability ended came back as current state with an empty
/// field beside it saying nothing had.
///
/// Reading this once, from the bundle, gives both verbs the same answer.
#[derive(Debug, Default)]
pub(super) struct MemoryLifecycle {
    /// The latest instant anywhere in this bundle: the store's own present.
    /// A kernel that must answer identically on every run cannot read the
    /// wall clock, and the frontier of what memory knows is the honest
    /// stand-in for now.
    frontier: Option<(i64, i32)>,
    expired: BTreeMap<String, Option<Timestamp>>,
    superseded: BTreeSet<String>,
}

impl MemoryLifecycle {
    pub(super) fn read(bundle: &KmpBundle) -> Self {
        let frontier = bundle
            .relationships()
            .iter()
            .flat_map(|relationship| {
                let explanation = relationship.explanation();
                [
                    instant(explanation.occurred_at()),
                    instant(explanation.observed_at()),
                    instant(explanation.ingested_at()),
                    instant(explanation.valid_from()),
                ]
            })
            .flatten()
            .max();

        let superseded = bundle
            .relationships()
            .iter()
            .filter(|relationship| relationship.relationship_type() == "supersedes")
            .map(|relationship| relationship.target_node_id().to_string())
            .collect();

        let expired = frontier
            .map(|frontier| {
                bundle
                    .relationships()
                    .iter()
                    .filter(|relationship| relationship.relationship_type() == "contains_entry")
                    .filter_map(|relationship| {
                        let valid_until = relationship.explanation().valid_until()?;
                        let ended = instant(Some(valid_until))?;
                        (ended < frontier).then(|| {
                            (
                                relationship.target_node_id().to_string(),
                                timestamp_from_sort_or_rfc3339(Some(valid_until)),
                            )
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            frontier,
            expired,
            superseded,
        }
    }

    /// The frontier is an internal notion: callers read the answers derived
    /// from it — expiry and recency — never the instant itself.
    #[cfg(test)]
    fn frontier(&self) -> Option<(i64, i32)> {
        self.frontier
    }

    pub(super) fn is_expired(&self, memory_ref: &str) -> bool {
        self.expired.contains_key(memory_ref)
    }

    pub(super) fn expired_refs(&self) -> impl Iterator<Item = &String> {
        self.expired.keys()
    }

    pub(super) fn superseded_refs(&self) -> &BTreeSet<String> {
        &self.superseded
    }

    pub(super) fn is_superseded(&self, memory_ref: &str) -> bool {
        self.superseded.contains(memory_ref)
    }

    /// The `proof.expired` list for the neighbourhood this response was built
    /// from.
    ///
    /// Scoped like `proof.superseded`, which is read from the relations in the
    /// response rather than from the citations that survived a cap. That
    /// matters most for Ask, which withholds an expired entry: reporting the
    /// expiry is what keeps the exclusion from being silent.
    pub(super) fn expired_memories(&self) -> Vec<ExpiredMemory> {
        self.expired
            .iter()
            .map(|(memory_ref, valid_until)| ExpiredMemory {
                r#ref: memory_ref.clone(),
                valid_until: *valid_until,
            })
            .collect()
    }

    /// How recent a memory is against the store's own present, in coarse
    /// buckets so a few seconds never outrank a stronger signal.
    ///
    /// A memory with no time is not treated as ancient: it ranks with old
    /// material rather than below it, because an absent clock is a silence,
    /// not a claim of age.
    pub(super) fn recency_rank(&self, time: Option<&Timestamp>) -> u32 {
        let (Some(frontier), Some(time)) = (self.frontier, time) else {
            return 1;
        };
        match frontier.0.saturating_sub(time.seconds) {
            age if age <= SECONDS_PER_DAY => 4,
            age if age <= 7 * SECONDS_PER_DAY => 3,
            age if age <= 30 * SECONDS_PER_DAY => 2,
            _ => 1,
        }
    }
}

/// Reads a stored instant in either shape the kernel writes it.
fn instant(value: Option<&str>) -> Option<(i64, i32)> {
    timestamp_from_sort_or_rfc3339(value).map(|time| (time.seconds, time.nanos))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap as StdMap;

    use kmp_domain::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, RelationExplanation,
        RelationSemanticClass, Role,
    };

    use super::*;

    fn node(id: &str) -> BundleNode {
        BundleNode::new(
            id,
            "memory",
            id,
            "fixture",
            "ACTIVE",
            Vec::new(),
            StdMap::new(),
        )
    }

    fn entry(target: &str, observed_at: &str, valid_until: Option<&str>) -> BundleRelationship {
        let mut explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension("timeline")
            .with_scope_id("scope:timeline")
            .with_observed_at(observed_at);
        if let Some(valid_until) = valid_until {
            explanation = explanation.with_valid_until(valid_until);
        }
        BundleRelationship::new("scope:timeline", target, "contains_entry", explanation)
    }

    fn bundle(relationships: Vec<BundleRelationship>, refs: &[&str]) -> KmpBundle {
        KmpBundle::new(
            CaseId::new("about:memory").expect("case id"),
            Role::new("resumer").expect("role"),
            node("about:memory"),
            refs.iter().map(|id| node(id)).collect(),
            relationships,
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("valid bundle")
    }

    #[test]
    fn an_applicability_that_ended_before_the_frontier_is_expired() {
        let lifecycle = MemoryLifecycle::read(&bundle(
            vec![
                entry(
                    "claim:expired",
                    "2026-01-01T00:00:00Z",
                    Some("2026-02-01T00:00:00Z"),
                ),
                entry("claim:live", "2026-03-01T00:00:00Z", None),
            ],
            &["scope:timeline", "claim:expired", "claim:live"],
        ));

        assert!(lifecycle.is_expired("claim:expired"));
        assert!(!lifecycle.is_expired("claim:live"));
    }

    #[test]
    fn an_applicability_that_still_runs_is_not_expired() {
        let lifecycle = MemoryLifecycle::read(&bundle(
            vec![
                entry(
                    "claim:open",
                    "2026-01-01T00:00:00Z",
                    Some("2026-12-31T00:00:00Z"),
                ),
                entry("claim:live", "2026-03-01T00:00:00Z", None),
            ],
            &["scope:timeline", "claim:open", "claim:live"],
        ));

        assert!(!lifecycle.is_expired("claim:open"));
    }

    #[test]
    fn every_expiry_in_the_neighbourhood_is_reported() {
        let lifecycle = MemoryLifecycle::read(&bundle(
            vec![
                entry(
                    "claim:expired",
                    "2026-01-01T00:00:00Z",
                    Some("2026-02-01T00:00:00Z"),
                ),
                entry(
                    "claim:also-expired",
                    "2026-01-01T00:00:00Z",
                    Some("2026-02-02T00:00:00Z"),
                ),
                entry("claim:live", "2026-03-01T00:00:00Z", None),
            ],
            &[
                "scope:timeline",
                "claim:expired",
                "claim:also-expired",
                "claim:live",
            ],
        ));

        let reported = lifecycle.expired_memories();

        assert_eq!(
            reported
                .iter()
                .map(|item| item.r#ref.as_str())
                .collect::<Vec<_>>(),
            vec!["claim:also-expired", "claim:expired"]
        );
        assert!(reported.iter().all(|item| item.valid_until.is_some()));
    }

    #[test]
    fn supersession_is_read_from_the_relation_that_declares_it() {
        let lifecycle = MemoryLifecycle::read(&bundle(
            vec![BundleRelationship::new(
                "claim:new",
                "claim:old",
                "supersedes",
                RelationExplanation::new(RelationSemanticClass::Evidential),
            )],
            &["claim:new", "claim:old"],
        ));

        assert!(lifecycle.is_superseded("claim:old"));
        assert!(!lifecycle.is_superseded("claim:new"));
        assert_eq!(lifecycle.superseded_refs().len(), 1);
    }

    #[test]
    fn a_store_with_no_clock_expires_nothing_and_ranks_every_memory_alike() {
        let lifecycle = MemoryLifecycle::read(&bundle(
            vec![BundleRelationship::new(
                "scope:timeline",
                "claim:undated",
                "contains_entry",
                RelationExplanation::new(RelationSemanticClass::Structural)
                    .with_valid_until("2026-02-01T00:00:00Z"),
            )],
            &["scope:timeline", "claim:undated"],
        ));

        assert!(lifecycle.frontier().is_none());
        assert!(!lifecycle.is_expired("claim:undated"));
        assert_eq!(lifecycle.recency_rank(None), 1);
        assert_eq!(
            lifecycle.recency_rank(Some(&Timestamp {
                seconds: 1_772_323_200,
                nanos: 0
            })),
            1
        );
    }

    #[test]
    fn recency_buckets_run_from_the_frontier_backwards() {
        let lifecycle = MemoryLifecycle::read(&bundle(
            vec![entry("claim:a", "2026-03-01T00:00:00Z", None)],
            &["scope:timeline", "claim:a"],
        ));
        let frontier = lifecycle.frontier().expect("frontier").0;
        let at = |age: i64| {
            lifecycle.recency_rank(Some(&Timestamp {
                seconds: frontier - age,
                nanos: 0,
            }))
        };

        assert_eq!(at(0), 4);
        assert_eq!(at(3 * SECONDS_PER_DAY), 3);
        assert_eq!(at(20 * SECONDS_PER_DAY), 2);
        assert_eq!(at(400 * SECONDS_PER_DAY), 1);
    }
}
