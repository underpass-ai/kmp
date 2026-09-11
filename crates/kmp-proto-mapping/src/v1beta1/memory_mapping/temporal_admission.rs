//! Which entries a recall bounded in time admits, and where it stands.
//!
//! A recall that names an instant or a span reads the bundle's coordinates
//! — the five clocks every `contains_entry` relation carries — and decides,
//! entry by entry on the clock the caller asked for, what competes. Evidence
//! follows the entries it supports, but an observed selection cannot admit
//! proof received after its end merely because it supports an older entry.
//! What this admits is decided before the ranker builds
//! its collection, so the statistics that weigh a word are the span's own.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{
    KmpBundle, TemporalAxis, TemporalCoordinate, TemporalCursor, TemporalSelection,
    compare_temporal_instants,
};
use kmp_proto::v1beta1::{MemoryEvidence, NearestOutside, TemporalInterval as ProtoInterval};
use prost_types::Timestamp;

use super::scalars::{
    ProtoMappingResult, invalid_argument, proto_temporal_axis, timestamp_from_sort_or_rfc3339,
};

/// The instant a lifecycle is read at, and whether that instant itself has
/// already passed: it has for "as of 03:10", and it has not for the
/// exclusive end of a span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LifecycleInstant {
    pub(super) at: (i64, i32),
    pub(super) inclusive: bool,
}

/// An entry's instant on the clock a recall reads, and which clock that
/// turned out to be — under the compatible precedence it differs per entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryInstant {
    instant: String,
    clock: TemporalAxis,
}

#[derive(Debug)]
pub(super) struct TemporalAdmission {
    selection: TemporalSelection,
    /// The instant an `as_of` cursor resolved to, in the store's spelling.
    resolved_as_of: Option<String>,
    /// The entries that compete; `None` when nothing bounds the recall.
    admitted: Option<BTreeSet<String>>,
    /// Every entry that stands somewhere in time, admitted or not.
    placed: BTreeSet<String>,
    /// The earliest instant each entry stands at on the clock read.
    instants_by_ref: BTreeMap<String, EntryInstant>,
    /// Which entries each evidence node supports, so proof follows its claim.
    supported_by: BTreeMap<String, Vec<String>>,
    /// Evidence with an explicit receipt time after the observed boundary.
    late_evidence: BTreeSet<String>,
}

impl TemporalAdmission {
    pub(super) fn read(
        bundle: &KmpBundle,
        selection: &TemporalSelection,
    ) -> ProtoMappingResult<Self> {
        let Some(axis) = selection.axis() else {
            return Ok(Self {
                selection: selection.clone(),
                resolved_as_of: None,
                admitted: None,
                placed: BTreeSet::new(),
                instants_by_ref: BTreeMap::new(),
                supported_by: BTreeMap::new(),
                late_evidence: BTreeSet::new(),
            });
        };
        let coordinates = coordinates_by_ref(bundle);
        let instants_by_ref = coordinates
            .iter()
            .filter_map(|(entry_ref, coordinates)| {
                earliest_instant(coordinates, axis).map(|instant| (entry_ref.clone(), instant))
            })
            .collect::<BTreeMap<_, _>>();

        let resolved_as_of = match selection.cursor() {
            Some(TemporalCursor::Time(time)) => Some(time.clone()),
            Some(TemporalCursor::Ref(entry_ref)) => Some(
                instants_by_ref
                    .get(entry_ref)
                    .map(|instant| instant.instant.clone())
                    .ok_or_else(|| {
                        invalid_argument(format!(
                            "as_of.ref `{entry_ref}` is not in the memory read for this \
                             question, or carries no instant on the {} clock",
                            axis_name(axis)
                        ))
                    })?,
            ),
            Some(TemporalCursor::Sequence(_)) | None => None,
        };

        let window = kmp_domain::TemporalReadWindow::new(selection, resolved_as_of.as_deref());
        let admitted = coordinates
            .iter()
            .filter(|(_, coordinates)| {
                coordinates
                    .iter()
                    .any(|coordinate| window.admits_coordinate(coordinate))
            })
            .map(|(entry_ref, _)| entry_ref.clone())
            .collect::<BTreeSet<_>>();

        let observed_end = (axis == TemporalAxis::Observed)
            .then(|| match selection {
                TemporalSelection::AsOf { .. } => resolved_as_of.as_deref().map(|at| (at, true)),
                TemporalSelection::Within { interval, .. } => interval.end().map(|at| (at, false)),
                TemporalSelection::Frontier => None,
            })
            .flatten();
        let late_evidence = std::iter::once(bundle.root_node())
            .chain(bundle.neighbor_nodes())
            .filter(|node| matches!(node.node_kind(), "memory_evidence" | "evidence"))
            .filter(|node| {
                let Some((end, inclusive)) = observed_end else {
                    return false;
                };
                let Some(time) = node.properties().get("payload_time") else {
                    return false;
                };
                matches!(
                    compare_temporal_instants(time, end),
                    Some(Ordering::Greater)
                ) || (!inclusive && compare_temporal_instants(time, end) == Some(Ordering::Equal))
            })
            .map(|node| node.node_id().to_string())
            .collect();

        let mut admission = Self {
            selection: selection.clone(),
            resolved_as_of,
            admitted: Some(admitted),
            placed: coordinates.into_keys().collect(),
            instants_by_ref,
            supported_by: BTreeMap::new(),
            late_evidence,
        };
        // Keep a key even when no association survives. Candidate.supports is
        // from the current payload and must not bypass the historical edges.
        let window = kmp_domain::TemporalReadWindow::new(
            &admission.selection,
            admission.resolved_as_of.as_deref(),
        );
        for edge in bundle
            .relationships()
            .iter()
            .filter(|e| e.relationship_type() == "supports")
        {
            let admitted = !admission.excludes(edge.source_node_id())
                && !admission.excludes(edge.target_node_id())
                && window.admits_relation_clock(edge.explanation());
            let targets = admission
                .supported_by
                .entry(edge.source_node_id().to_owned())
                .or_default();
            if admitted {
                targets.push(edge.target_node_id().to_owned());
            }
        }
        Ok(admission)
    }

    /// Whether an entry, by its own coordinates, falls inside the selection.
    pub(super) fn admits_entry(&self, entry_ref: &str) -> bool {
        self.admitted
            .as_ref()
            .is_none_or(|admitted| admitted.contains(entry_ref))
    }

    /// Of the abouts read, those with no entry the selection admits: silence
    /// from an about that was looked at, said apart from absence. Empty when
    /// nothing bounds the recall.
    pub(super) fn abouts_empty_in_selection(
        &self,
        abouts: &[String],
        about_by_entry: &BTreeMap<String, String>,
    ) -> Vec<String> {
        let Some(admitted) = &self.admitted else {
            return Vec::new();
        };
        let with_something = admitted
            .iter()
            .filter_map(|entry| about_by_entry.get(entry))
            .collect::<BTreeSet<_>>();
        abouts
            .iter()
            .filter(|about| !with_something.contains(about))
            .cloned()
            .collect()
    }

    pub(super) fn admits_coordinate(&self, coordinate: &TemporalCoordinate) -> bool {
        kmp_domain::TemporalReadWindow::new(&self.selection, self.resolved_as_of.as_deref())
            .admits_coordinate(coordinate)
    }

    /// Whether a node stands in time outside the selection: an entry with a
    /// coordinate that the selection did not admit. What touches such a
    /// node did not exist where the recall stands.
    pub(super) fn excludes(&self, node_ref: &str) -> bool {
        self.late_evidence.contains(node_ref)
            || self.admitted.as_ref().is_some_and(|admitted| {
                self.placed.contains(node_ref) && !admitted.contains(node_ref)
            })
    }

    /// The bundle as it stood where the recall stands: every relation that
    /// touches an entry outside the selection or is explicitly later on the
    /// selected clock is left out, so a replacement
    /// that did not exist yet replaces nothing, a proof path never crosses
    /// into what came later, and the graph the ranker reaches through is the
    /// selection's. Nodes and details stay, so what lies outside can still
    /// be ranked for `nearest_outside`. Unbounded, the bundle is borrowed.
    pub(super) fn bound<'a>(&self, bundle: &'a KmpBundle) -> Cow<'a, KmpBundle> {
        if self.admitted.is_none() {
            return Cow::Borrowed(bundle);
        }
        let window =
            kmp_domain::TemporalReadWindow::new(&self.selection, self.resolved_as_of.as_deref());
        let relationships = bundle
            .relationships()
            .iter()
            .filter(|relationship| {
                !self.excludes(relationship.source_node_id())
                    && !self.excludes(relationship.target_node_id())
                    && window.admits_relation_clock(relationship.explanation())
            })
            .cloned()
            .collect();
        match KmpBundle::new(
            bundle.root_node_id().clone(),
            bundle.role().clone(),
            bundle.root_node().clone(),
            bundle.neighbor_nodes().to_vec(),
            relationships,
            bundle.node_details().to_vec(),
            bundle.metadata().clone(),
        ) {
            Ok(bounded) => Cow::Owned(bounded),
            // A bundle the kernel already accepted is valid with fewer edges;
            // if it were not, standing on the whole is the honest fallback.
            Err(_) => Cow::Borrowed(bundle),
        }
    }

    /// Dependency traversal may use a declared validity interval only where
    /// it overlaps this selection. Historical proof paths remain unchanged.
    pub(super) fn admits_dependency_relation(
        &self,
        relationship: &kmp_domain::BundleRelationship,
    ) -> bool {
        kmp_domain::TemporalReadWindow::new(&self.selection, self.resolved_as_of.as_deref())
            .admits_dependency_relation(relationship.explanation())
    }

    /// Whether the recall is bounded to a span — the one case an UNKNOWN can
    /// name what lies nearest outside.
    pub(super) fn bounds_a_span(&self) -> bool {
        self.selection.interval().is_some()
    }

    /// The instant the lifecycles are read at, when the selection names one.
    /// A span with an open end stands at the memory's frontier.
    pub(super) fn lifecycle_instant(&self) -> Option<LifecycleInstant> {
        match &self.selection {
            TemporalSelection::Frontier => None,
            TemporalSelection::AsOf { .. } => {
                let at = timestamp_from_sort_or_rfc3339(self.resolved_as_of.as_deref())?;
                Some(LifecycleInstant {
                    at: (at.seconds, at.nanos),
                    inclusive: true,
                })
            }
            TemporalSelection::Within { interval, .. } => {
                let end = timestamp_from_sort_or_rfc3339(interval.end())?;
                Some(LifecycleInstant {
                    at: (end.seconds, end.nanos),
                    inclusive: false,
                })
            }
        }
    }

    pub(super) fn axis(&self) -> TemporalAxis {
        self.selection.axis().unwrap_or_default()
    }

    /// Whether a candidate competes: an entry by its own coordinates, an
    /// evidence node through the entries it supports.
    pub(super) fn admits(&self, item: &MemoryEvidence) -> bool {
        let Some(admitted) = &self.admitted else {
            return true;
        };
        let evidence_ref = item.id.strip_prefix("detail:").unwrap_or(&item.id);
        if self.late_evidence.contains(evidence_ref) {
            return false;
        }
        if self.placed.contains(evidence_ref) {
            return admitted.contains(evidence_ref);
        }
        if let Some(targets) = self.supported_by.get(evidence_ref) {
            return targets.iter().any(|target| admitted.contains(target));
        }
        candidate_refs(item)
            .into_iter()
            .any(|candidate_ref| self.admits_ref(admitted, &candidate_ref))
    }

    /// An admitted source may still have associations outside this cut.
    pub(super) fn bound_supports(&self, item: &mut MemoryEvidence) {
        if self.admitted.is_none() {
            return;
        }
        let source = item.id.strip_prefix("detail:").unwrap_or(&item.id);
        if self.placed.contains(source) {
            return;
        }
        if let Some(targets) = self.supported_by.get(source) {
            item.supports.retain(|target| targets.contains(target));
        }
    }

    fn admits_ref(&self, admitted: &BTreeSet<String>, candidate_ref: &str) -> bool {
        admitted.contains(candidate_ref)
            || self
                .supported_by
                .get(candidate_ref)
                .is_some_and(|entries| entries.iter().any(|entry| admitted.contains(entry)))
    }

    /// Among candidates that lie outside the span, the one standing closest
    /// to it: the difference between "not known" and "not then".
    pub(super) fn nearest_outside(&self, outside: &[MemoryEvidence]) -> Option<NearestOutside> {
        let interval = self.selection.interval()?;
        outside
            .iter()
            .flat_map(candidate_refs)
            .filter_map(|entry_ref| {
                let instant = self.instants_by_ref.get(&entry_ref)?;
                let distance = interval.distance_outside(&instant.instant)?;
                (distance > 0).then_some((distance, entry_ref, instant))
            })
            .min_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)))
            .map(|(_, entry_ref, instant)| NearestOutside {
                r#ref: entry_ref,
                time: timestamp_from_sort_or_rfc3339(Some(&instant.instant)),
                axis: proto_temporal_axis(instant.clock) as i32,
            })
    }

    /// What the proof declares about where this recall stood.
    pub(super) fn proof_fields(&self) -> ProofTemporalFields {
        match &self.selection {
            TemporalSelection::Frontier => ProofTemporalFields::default(),
            TemporalSelection::AsOf { axis, .. } => ProofTemporalFields {
                interval: None,
                axis: proto_temporal_axis(*axis) as i32,
                as_of: timestamp_from_sort_or_rfc3339(self.resolved_as_of.as_deref()),
            },
            TemporalSelection::Within { interval, axis } => ProofTemporalFields {
                interval: Some(ProtoInterval {
                    start: timestamp_from_sort_or_rfc3339(interval.start()),
                    end: timestamp_from_sort_or_rfc3339(interval.end()),
                }),
                axis: proto_temporal_axis(*axis) as i32,
                as_of: None,
            },
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ProofTemporalFields {
    pub(super) interval: Option<ProtoInterval>,
    pub(super) axis: i32,
    pub(super) as_of: Option<Timestamp>,
}

/// The entry refs a candidate stands for: an entry candidate names its own
/// entry, an evidence candidate the entries it supports.
fn candidate_refs(item: &MemoryEvidence) -> Vec<String> {
    if let Some(entry_ref) = item.id.strip_prefix("entry:") {
        return vec![entry_ref.to_string()];
    }
    let mut refs = item.supports.clone();
    if let Some(detail_ref) = item.id.strip_prefix("detail:") {
        refs.push(detail_ref.to_string());
    }
    refs
}

pub(super) fn coordinates_by_ref(bundle: &KmpBundle) -> BTreeMap<String, Vec<TemporalCoordinate>> {
    let mut coordinates = BTreeMap::<String, Vec<TemporalCoordinate>>::new();
    for relationship in bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
    {
        if let Ok(Some(coordinate)) =
            TemporalCoordinate::from_relation_explanation(relationship.explanation())
        {
            coordinates
                .entry(relationship.target_node_id().to_string())
                .or_default()
                .push(coordinate);
        }
    }
    coordinates
}

/// The instant one coordinate stands at on the clock read. An explicit clock
/// never substitutes another; the compatible precedence resolves to the
/// first clock the coordinate carries and says which.
pub(super) use kmp_domain::temporal_clock_instant as clock_instant;

fn earliest_instant(
    coordinates: &[TemporalCoordinate],
    axis: TemporalAxis,
) -> Option<EntryInstant> {
    coordinates
        .iter()
        .filter_map(|coordinate| clock_instant(coordinate, axis))
        .min_by(|left, right| compare_temporal_instants(left.0, right.0).unwrap_or(Ordering::Equal))
        .map(|(instant, clock)| EntryInstant {
            instant: instant.to_string(),
            clock,
        })
}

fn axis_name(axis: TemporalAxis) -> &'static str {
    match axis {
        TemporalAxis::Default => "default",
        TemporalAxis::Occurred => "occurred",
        TemporalAxis::Observed => "observed",
        TemporalAxis::Ingested => "ingested",
        TemporalAxis::Validity => "validity",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap as StdMap;

    use kmp_domain::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, RelationExplanation,
        RelationSemanticClass, Role, TemporalInterval,
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

    fn entry(
        target: &str,
        occurred_at: Option<&str>,
        observed_at: Option<&str>,
    ) -> BundleRelationship {
        let mut explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension("work")
            .with_scope_id("scope:work");
        if let Some(at) = occurred_at {
            explanation = explanation.with_occurred_at(at);
        }
        if let Some(at) = observed_at {
            explanation = explanation.with_observed_at(at);
        }
        BundleRelationship::new("scope:work", target, "contains_entry", explanation)
    }

    fn supports(evidence: &str, entry: &str) -> BundleRelationship {
        BundleRelationship::new(
            evidence,
            entry,
            "supports",
            RelationExplanation::new(RelationSemanticClass::Evidential),
        )
    }

    fn bundle(relationships: Vec<BundleRelationship>, refs: &[&str]) -> KmpBundle {
        KmpBundle::new(
            CaseId::new("about:x").expect("case id"),
            Role::new("answerer").expect("role"),
            node("about:x"),
            refs.iter().map(|id| node(id)).collect(),
            relationships,
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("valid bundle")
    }

    fn candidate(id: &str, supports: &[&str]) -> MemoryEvidence {
        MemoryEvidence {
            support_clocks: None,
            id: id.to_string(),
            supports: supports.iter().map(|s| s.to_string()).collect(),
            text: String::new(),
            source: String::new(),
            time: None,
            metadata: Default::default(),
        }
    }

    fn march() -> TemporalInterval {
        TemporalInterval::new(
            Some("2026-03-01T00:00:00Z".to_string()),
            Some("2026-04-01T00:00:00Z".to_string()),
        )
        .expect("march")
    }

    /// Inside the span an entry competes by its own clock; evidence follows
    /// the entry it supports; and an explicit clock the entry lacks admits
    /// nothing.
    #[test]
    fn an_entry_supporting_a_future_entry_keeps_its_own_temporal_identity() {
        let bundle = bundle(
            vec![
                entry("e:march", Some("2026-03-10T00:00:00Z"), None),
                entry("e:april", Some("2026-04-10T00:00:00Z"), None),
                supports("e:march", "e:april"),
            ],
            &["scope:work", "e:march", "e:april"],
        );
        let admission = TemporalAdmission::read(
            &bundle,
            &TemporalSelection::within(march(), TemporalAxis::Occurred),
        )
        .expect("selection");
        let mut item = candidate("detail:e:march", &["e:march"]);
        assert!(admission.admits(&item));
        admission.bound_supports(&mut item);
        assert_eq!(item.supports, vec!["e:march"]);
    }

    #[test]
    fn a_span_admits_by_the_clock_read_and_proof_follows_its_entry() {
        let bundle = bundle(
            vec![
                entry(
                    "e:march",
                    Some("2026-03-10T00:00:00Z"),
                    Some("2026-04-12T00:00:00Z"),
                ),
                entry("e:april", Some("2026-04-20T00:00:00Z"), None),
                supports("ev:march", "e:march"),
            ],
            &["scope:work", "e:march", "e:april", "ev:march"],
        );
        let by_occurrence = TemporalAdmission::read(
            &bundle,
            &TemporalSelection::within(march(), TemporalAxis::Occurred),
        )
        .expect("admission");
        assert!(by_occurrence.admits(&candidate("entry:e:march", &["e:march"])));
        assert!(!by_occurrence.admits(&candidate("entry:e:april", &["e:april"])));
        assert!(
            by_occurrence.admits(&candidate("detail:ev:march", &["e:march"])),
            "evidence for a memory inside the span is inside the span"
        );

        let by_observation = TemporalAdmission::read(
            &bundle,
            &TemporalSelection::within(march(), TemporalAxis::Observed),
        )
        .expect("admission");
        assert!(
            !by_observation.admits(&candidate("entry:e:march", &["e:march"])),
            "seen in April, not in March"
        );
        assert!(
            !by_observation.admits(&candidate("entry:e:april", &["e:april"])),
            "never observed: an explicit clock never substitutes another"
        );
    }

    #[test]
    fn as_of_by_ref_stands_at_that_entry_and_an_unknown_ref_is_refused() {
        let bundle = bundle(
            vec![
                entry("e:first", Some("2026-03-10T00:00:00Z"), None),
                entry("e:second", Some("2026-03-20T00:00:00Z"), None),
            ],
            &["scope:work", "e:first", "e:second"],
        );
        let selection = TemporalSelection::as_of(
            TemporalCursor::ref_id("e:first").expect("ref"),
            TemporalAxis::Default,
        )
        .expect("as of a ref");
        let admission = TemporalAdmission::read(&bundle, &selection).expect("admission");
        assert!(admission.admits(&candidate("entry:e:first", &["e:first"])));
        assert!(!admission.admits(&candidate("entry:e:second", &["e:second"])));
        let stood = admission.proof_fields();
        assert_eq!(
            stood.as_of.map(|at| at.to_string()).as_deref(),
            Some("2026-03-10T00:00:00Z")
        );
        assert!(stood.interval.is_none());

        let missing = TemporalSelection::as_of(
            TemporalCursor::ref_id("e:absent").expect("ref"),
            TemporalAxis::Default,
        )
        .expect("as of a ref");
        let error = TemporalAdmission::read(&bundle, &missing).expect_err("not in the memory read");
        assert!(error.message().contains("e:absent"), "{}", error.message());
    }

    #[test]
    fn the_nearest_outside_is_the_closest_to_either_bound_and_names_its_clock() {
        let bundle = bundle(
            vec![
                entry("e:february", Some("2026-02-20T00:00:00Z"), None),
                entry("e:january", Some("2026-01-05T00:00:00Z"), None),
                entry("e:may", Some("2026-05-02T00:00:00Z"), None),
            ],
            &["scope:work", "e:february", "e:january", "e:may"],
        );
        let admission = TemporalAdmission::read(
            &bundle,
            &TemporalSelection::within(march(), TemporalAxis::Default),
        )
        .expect("admission");
        assert!(admission.bounds_a_span());
        let nearest = admission
            .nearest_outside(&[
                candidate("entry:e:january", &["e:january"]),
                candidate("entry:e:may", &["e:may"]),
                candidate("entry:e:february", &["e:february"]),
            ])
            .expect("something lies outside");
        assert_eq!(nearest.r#ref, "e:february");
        assert_eq!(
            nearest.axis,
            proto_temporal_axis(TemporalAxis::Occurred) as i32,
            "the precedence resolved to the occurred clock and says so"
        );
        assert_eq!(
            nearest.time.map(|at| at.to_string()).as_deref(),
            Some("2026-02-20T00:00:00Z")
        );
    }
}
