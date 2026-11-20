use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{KmpBundle, TemporalAxis, TemporalCoordinate};
use kmp_proto::v1beta1::ExpiredMemory;
use prost_types::Timestamp;

use super::relation_clock::present_of;
use super::scalars::timestamp_from_sort_or_rfc3339;
use super::temporal_admission::{LifecycleInstant, clock_instant};

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
    /// When each entry comes into force, from its `valid_from`.
    starts: BTreeMap<String, (i64, i32)>,
    /// Entries whose `valid_from` is still ahead of the instant this reads
    /// at: recorded, but not yet in force.
    pending: BTreeSet<String>,
    /// Whether the caller named the instant this was read at (`as_of`, an
    /// interval): a question standing in the past asks about history.
    at_an_instant: bool,
}

impl MemoryLifecycle {
    pub(super) fn read(bundle: &KmpBundle) -> Self {
        let frontier = bundle
            .relationships()
            .iter()
            .filter_map(|relationship| present_of(relationship.explanation()))
            .max();
        let starts = starts_of(bundle);

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

        let pending = pending_at(&starts, frontier, |start, frontier| start > frontier);
        Self {
            frontier,
            expired,
            superseded,
            starts,
            pending,
            at_an_instant: false,
        }
    }

    /// The lifecycle read from the candidates an ask read, standing on the
    /// frontier and the expiries the whole about declares (DESIGN L6, P13):
    /// what was replaced is read from the relations the ask read, which
    /// hold every replacement of a memory it read.
    pub(super) fn with_indexed(
        mut self,
        indexed: &super::indexed_lifecycle::IndexedLifecycle,
    ) -> Self {
        self.frontier = indexed.frontier();
        self.expired = indexed.expired();
        self.pending = pending_at(&self.starts, self.frontier, |start, frontier| {
            start > frontier
        });
        self
    }

    /// The lifecycles as they stood at one instant rather than at the
    /// memory's frontier: an entry is replaced only if its replacement
    /// already existed then, on the clock the recall reads, and expired only
    /// if its validity had ended by then. What was replaced or ran out
    /// *after* the instant is current for that question, and recency is
    /// measured from the instant.
    pub(super) fn read_at(
        bundle: &KmpBundle,
        instant: LifecycleInstant,
        axis: TemporalAxis,
    ) -> Self {
        let at = instant.at;
        // "Already" is `<=` when the instant itself has passed and `<` at
        // the exclusive end of a span.
        let already = |when: (i64, i32)| {
            if instant.inclusive {
                when <= at
            } else {
                when < at
            }
        };

        let mut instants_by_ref = BTreeMap::<String, (i64, i32)>::new();
        for relationship in bundle
            .relationships()
            .iter()
            .filter(|relationship| relationship.relationship_type() == "contains_entry")
        {
            let Ok(Some(coordinate)) =
                TemporalCoordinate::from_relation_explanation(relationship.explanation())
            else {
                continue;
            };
            let Some(when) =
                clock_instant(&coordinate, axis).and_then(|(when, _)| instant_of(when))
            else {
                continue;
            };
            let entry = instants_by_ref
                .entry(relationship.target_node_id().to_string())
                .or_insert(when);
            *entry = (*entry).min(when);
        }

        // A replacement whose instant is unknown counts, as it does at the
        // frontier: an absent clock is a silence, not a claim of lateness.
        let superseded = bundle
            .relationships()
            .iter()
            .filter(|relationship| relationship.relationship_type() == "supersedes")
            .filter(|relationship| {
                instants_by_ref
                    .get(relationship.source_node_id())
                    .is_none_or(|when| already(*when))
            })
            .map(|relationship| relationship.target_node_id().to_string())
            .collect();

        let expired = bundle
            .relationships()
            .iter()
            .filter(|relationship| relationship.relationship_type() == "contains_entry")
            .filter_map(|relationship| {
                let valid_until = relationship.explanation().valid_until()?;
                let ended = instant_of(valid_until)?;
                already(ended).then(|| {
                    (
                        relationship.target_node_id().to_string(),
                        timestamp_from_sort_or_rfc3339(Some(valid_until)),
                    )
                })
            })
            .collect();

        let starts = starts_of(bundle);
        let pending = pending_at(&starts, Some(at), |start, _| !already(start));
        Self {
            frontier: Some(at),
            expired,
            superseded,
            starts,
            pending,
            at_an_instant: true,
        }
    }

    /// The frontier is an internal notion: callers read the answers derived
    /// from it — expiry and recency — never the instant itself.
    #[cfg(test)]
    fn frontier(&self) -> Option<(i64, i32)> {
        self.frontier
    }

    /// Whether this lifecycle stands at an instant the caller named rather
    /// than at the memory's frontier.
    pub(super) fn reads_an_instant(&self) -> bool {
        self.at_an_instant
    }

    pub(super) fn is_expired(&self, memory_ref: &str) -> bool {
        self.expired.contains_key(memory_ref)
    }

    pub(super) fn expired_refs(&self) -> impl Iterator<Item = &String> {
        self.expired.keys()
    }

    /// Recorded, but its `valid_from` is still ahead: not yet in force.
    pub(super) fn is_pending(&self, memory_ref: &str) -> bool {
        self.pending.contains(memory_ref)
    }

    pub(super) fn pending_refs(&self) -> impl Iterator<Item = &String> {
        self.pending.iter()
    }

    /// Neither replaced, nor run out, nor still to come: current advice.
    pub(super) fn stands(&self, memory_ref: &str) -> bool {
        !self.is_superseded(memory_ref)
            && !self.is_expired(memory_ref)
            && !self.is_pending(memory_ref)
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

/// When each entry of the bundle comes into force: the earliest `valid_from`
/// its `contains_entry` edges name.
fn starts_of(bundle: &KmpBundle) -> BTreeMap<String, (i64, i32)> {
    let mut starts = BTreeMap::<String, (i64, i32)>::new();
    for relationship in bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
    {
        let Some(start) = instant(relationship.explanation().valid_from()) else {
            continue;
        };
        let earliest = starts
            .entry(relationship.target_node_id().to_string())
            .or_insert(start);
        *earliest = (*earliest).min(start);
    }
    starts
}

fn pending_at(
    starts: &BTreeMap<String, (i64, i32)>,
    at: Option<(i64, i32)>,
    ahead: impl Fn((i64, i32), (i64, i32)) -> bool,
) -> BTreeSet<String> {
    let Some(at) = at else {
        return BTreeSet::new();
    };
    starts
        .iter()
        .filter(|(_, start)| ahead(**start, at))
        .map(|(memory_ref, _)| memory_ref.clone())
        .collect()
}

/// Reads a stored instant in either shape the kernel writes it.
fn instant(value: Option<&str>) -> Option<(i64, i32)> {
    timestamp_from_sort_or_rfc3339(value).map(|time| (time.seconds, time.nanos))
}

fn instant_of(value: &str) -> Option<(i64, i32)> {
    instant(Some(value))
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

    fn dated(target: &str, observed: &str, from: &str, until: Option<&str>) -> BundleRelationship {
        let mut explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension("timeline")
            .with_scope_id("scope:timeline")
            .with_observed_at(observed)
            .with_valid_from(from);
        if let Some(until) = until {
            explanation = explanation.with_valid_until(until);
        }
        BundleRelationship::new("scope:timeline", target, "contains_entry", explanation)
    }

    /// B4 of the 1.0 review: a memory whose validity starts next year used to
    /// move the about's "now" to next year, so the memory in force this year
    /// was served as expired with high confidence.
    #[test]
    fn a_future_validity_neither_moves_now_nor_counts_as_current() {
        let rota = bundle(
            vec![
                dated(
                    "claim:marta",
                    "2026-11-01T10:00:00Z",
                    "2026-01-01T00:00:00Z",
                    Some("2026-12-31T23:59:59Z"),
                ),
                dated(
                    "claim:diego",
                    "2026-11-01T10:05:00Z",
                    "2027-01-01T00:00:00Z",
                    None,
                ),
            ],
            &["scope:timeline", "claim:marta", "claim:diego"],
        );
        let lifecycle = MemoryLifecycle::read(&rota);
        assert_eq!(
            lifecycle.frontier(),
            instant(Some("2026-11-01T10:05:00Z")),
            "the present is the latest observation, not a validity ahead of it"
        );
        assert!(!lifecycle.is_expired("claim:marta"));
        assert!(lifecycle.stands("claim:marta"));
        assert!(lifecycle.is_pending("claim:diego"));
        assert!(!lifecycle.stands("claim:diego"));

        // Asked as of next February, Diego is in force and Marta ran out.
        let later = MemoryLifecycle::read_at(
            &rota,
            LifecycleInstant {
                at: instant(Some("2027-02-01T00:00:00Z")).expect("instant"),
                inclusive: true,
            },
            TemporalAxis::Observed,
        );
        assert!(later.is_expired("claim:marta"));
        assert!(later.stands("claim:diego"));
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

    fn supersession(replacer: &str, replaced: &str) -> BundleRelationship {
        BundleRelationship::new(
            replacer,
            replaced,
            "supersedes",
            RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("replaced after review"),
        )
    }

    /// Read at an instant, a replacement that did not exist yet does not
    /// replace, and a validity that had not ended yet has not expired.
    #[test]
    fn read_at_an_instant_keeps_what_was_replaced_or_ran_out_only_later() {
        let bundle = bundle(
            vec![
                entry("claim:old", "2026-03-01T00:00:00Z", None),
                entry("claim:new", "2026-03-20T00:00:00Z", None),
                supersession("claim:new", "claim:old"),
                entry(
                    "claim:lease",
                    "2026-03-01T00:00:00Z",
                    Some("2026-04-01T00:00:00Z"),
                ),
                entry("claim:late", "2026-05-01T00:00:00Z", None),
            ],
            &[
                "scope:timeline",
                "claim:old",
                "claim:new",
                "claim:lease",
                "claim:late",
            ],
        );
        let at_frontier = MemoryLifecycle::read(&bundle);
        assert!(at_frontier.is_superseded("claim:old"));
        assert!(at_frontier.is_expired("claim:lease"));

        let march_tenth = timestamp_from_sort_or_rfc3339(Some("2026-03-10T00:00:00Z")).expect("t");
        let then = MemoryLifecycle::read_at(
            &bundle,
            LifecycleInstant {
                at: (march_tenth.seconds, march_tenth.nanos),
                inclusive: true,
            },
            TemporalAxis::Observed,
        );
        assert!(
            !then.is_superseded("claim:old"),
            "the replacement did not exist on the tenth"
        );
        assert!(
            !then.is_expired("claim:lease"),
            "the lease still ran on the tenth"
        );
        assert_eq!(
            then.recency_rank(Some(&march_tenth)),
            4,
            "recency is measured from the instant"
        );
    }

    /// The exclusive end of a span: a validity ending exactly there has not
    /// ended within the span; as of that very instant, it has.
    #[test]
    fn the_end_of_a_span_is_exclusive_and_an_instant_is_inclusive() {
        let bundle = bundle(
            vec![entry(
                "claim:lease",
                "2026-03-01T00:00:00Z",
                Some("2026-04-01T00:00:00Z"),
            )],
            &["scope:timeline", "claim:lease"],
        );
        let april = timestamp_from_sort_or_rfc3339(Some("2026-04-01T00:00:00Z")).expect("t");
        let at = (april.seconds, april.nanos);
        let within = MemoryLifecycle::read_at(
            &bundle,
            LifecycleInstant {
                at,
                inclusive: false,
            },
            TemporalAxis::Default,
        );
        assert!(!within.is_expired("claim:lease"));
        let as_of = MemoryLifecycle::read_at(
            &bundle,
            LifecycleInstant {
                at,
                inclusive: true,
            },
            TemporalAxis::Default,
        );
        assert!(as_of.is_expired("claim:lease"));
    }
}
