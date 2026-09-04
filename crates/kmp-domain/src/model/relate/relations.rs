use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::{TemporalAxis, compare_temporal_instants};

use super::coordinate_relation::{CoordinateRelation, CoordinateRelationKind};
use super::declared_edge::DeclaredEdge;
use super::related_fact::{FactState, RelatedFact};
use super::shared_label::SharedLabel;
use super::tension::Tension;

/// The most coordinate relations one reading returns. Pairs grow with the
/// square of the facts a scope holds; past this many the reading says how
/// many it left out rather than growing without bound.
pub const MAX_COORDINATE_RELATIONS: usize = 500;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Relations {
    pub coordinate: Vec<CoordinateRelation>,
    pub tensions: Vec<Tension>,
    /// Coordinate relations past the cap, counted and not returned.
    pub omitted_coordinate: usize,
}

/// Reads what facts of different abouts have to do with each other, and
/// which declared contradictions still stand between facts that do.
///
/// Coordinate relations are read inside each label two or more abouts
/// share — the same dimension kind and the same scope — one per pair of
/// facts from different abouts, ordered by label and then by the pair, so
/// the same store reads the same way every time.
pub fn relate(facts: &[RelatedFact], declared: &[DeclaredEdge], axis: TemporalAxis) -> Relations {
    let mut by_label = BTreeMap::<SharedLabel, Vec<&RelatedFact>>::new();
    for fact in facts {
        for label in fact.labels() {
            by_label.entry(label).or_default().push(fact);
        }
    }

    let mut coordinate = Vec::new();
    let mut omitted_coordinate = 0usize;
    for (label, mut placed) in by_label {
        placed.sort_by(|left, right| {
            (left.about(), left.ref_id()).cmp(&(right.about(), right.ref_id()))
        });
        placed.dedup_by(|left, right| left.ref_id() == right.ref_id());
        for (index, first) in placed.iter().enumerate() {
            for second in &placed[index + 1..] {
                if first.about() == second.about() {
                    continue;
                }
                for relation in pair_relations(first, second, &label, axis) {
                    if coordinate.len() < MAX_COORDINATE_RELATIONS {
                        coordinate.push(relation);
                    } else {
                        omitted_coordinate += 1;
                    }
                }
            }
        }
    }

    let by_ref = facts
        .iter()
        .map(|fact| (fact.ref_id(), fact))
        .collect::<BTreeMap<_, _>>();
    let mut tensions = declared
        .iter()
        .filter(|edge| edge.rel == "contradicts")
        .filter_map(|edge| {
            let first = by_ref.get(edge.from.as_str())?;
            let second = by_ref.get(edge.to.as_str())?;
            if first.state() != FactState::Current || second.state() != FactState::Current {
                return None;
            }
            let shared = first
                .labels()
                .intersection(&second.labels())
                .next()
                .cloned();
            Some(Tension::new(
                edge.from.clone(),
                edge.to.clone(),
                shared,
                edge.why.clone(),
                edge.evidence.clone(),
            ))
        })
        .collect::<Vec<_>>();
    tensions
        .sort_by(|left, right| (left.ref_id(), left.other()).cmp(&(right.ref_id(), right.other())));
    tensions.dedup();

    Relations {
        coordinate,
        tensions,
        omitted_coordinate,
    }
}

fn pair_relations(
    first: &RelatedFact,
    second: &RelatedFact,
    label: &SharedLabel,
    axis: TemporalAxis,
) -> Vec<CoordinateRelation> {
    let mut relations = Vec::new();
    let temporal = if axis == TemporalAxis::Validity {
        span_relation(first.validity_in(label), second.validity_in(label))
    } else {
        instant_relation(
            first.instant_in(label, axis),
            second.instant_in(label, axis),
        )
    };
    let (from, to, kind) = match temporal {
        Some((kind, swapped)) if swapped => (second, first, kind),
        Some((kind, _)) => (first, second, kind),
        None => (first, second, CoordinateRelationKind::SharesScope),
    };
    relations.push(CoordinateRelation::new(
        from.ref_id(),
        to.ref_id(),
        kind,
        label,
        axis,
    ));
    if let (Some(left), Some(right)) = (first.sequence_in(label), second.sequence_in(label))
        && left == right
    {
        relations.push(CoordinateRelation::new(
            first.ref_id(),
            second.ref_id(),
            CoordinateRelationKind::SameSequence,
            label,
            axis,
        ));
    }
    if let (Some(left), Some(right)) = (first.rank_in(label), second.rank_in(label))
        && left == right
    {
        relations.push(CoordinateRelation::new(
            first.ref_id(),
            second.ref_id(),
            CoordinateRelationKind::SameRank,
            label,
            axis,
        ));
    }
    relations
}

/// The order of two instants, from the first's point of view; `None` when
/// either is missing or unreadable, which is what `SharesScope` says.
fn instant_relation(
    first: Option<&str>,
    second: Option<&str>,
) -> Option<(CoordinateRelationKind, bool)> {
    let ordering = compare_temporal_instants(first?, second?)?;
    Some((
        match ordering {
            Ordering::Less => CoordinateRelationKind::Before,
            Ordering::Greater => CoordinateRelationKind::After,
            Ordering::Equal => CoordinateRelationKind::Concurrent,
        },
        false,
    ))
}

/// How two validity spans stand: nested reads as `During` from the inner
/// one (the second flag says the inner one was the second fact), touching
/// as `Concurrent`, and apart as `Before` or `After`. An open side is
/// unbounded on that side.
fn span_relation(
    first: Option<(Option<&str>, Option<&str>)>,
    second: Option<(Option<&str>, Option<&str>)>,
) -> Option<(CoordinateRelationKind, bool)> {
    let (first_from, first_until) = first?;
    let (second_from, second_until) = second?;
    let starts_not_after = |a: Option<&str>, b: Option<&str>| match (a, b) {
        (None, _) => Some(true),
        (Some(_), None) => Some(false),
        (Some(a), Some(b)) => Some(compare_temporal_instants(a, b)? != Ordering::Greater),
    };
    let ends_not_before = |a: Option<&str>, b: Option<&str>| match (a, b) {
        (None, _) => Some(true),
        (Some(_), None) => Some(false),
        (Some(a), Some(b)) => Some(compare_temporal_instants(a, b)? != Ordering::Less),
    };
    let apart_before = match (first_until, second_from) {
        (Some(until), Some(from)) => compare_temporal_instants(until, from)? != Ordering::Greater,
        _ => false,
    };
    if apart_before {
        return Some((CoordinateRelationKind::Before, false));
    }
    let apart_after = match (second_until, first_from) {
        (Some(until), Some(from)) => compare_temporal_instants(until, from)? != Ordering::Greater,
        _ => false,
    };
    if apart_after {
        return Some((CoordinateRelationKind::After, false));
    }
    let first_inside =
        starts_not_after(second_from, first_from)? && ends_not_before(second_until, first_until)?;
    let second_inside =
        starts_not_after(first_from, second_from)? && ends_not_before(first_until, second_until)?;
    if first_inside && second_inside {
        return Some((CoordinateRelationKind::Concurrent, false));
    }
    if first_inside {
        return Some((CoordinateRelationKind::During, false));
    }
    if second_inside {
        return Some((CoordinateRelationKind::During, true));
    }
    Some((CoordinateRelationKind::Concurrent, false))
}

#[cfg(test)]
mod tests {
    use crate::{RelationExplanation, RelationSemanticClass, TemporalCoordinate};

    use super::*;

    fn coordinate(
        scope: &str,
        occurred_at: Option<&str>,
        validity: (Option<&str>, Option<&str>),
        sequence: Option<u32>,
    ) -> TemporalCoordinate {
        coordinate_of_kind("incident", scope, occurred_at, validity, sequence)
    }

    fn coordinate_of_kind(
        kind: &str,
        scope: &str,
        occurred_at: Option<&str>,
        validity: (Option<&str>, Option<&str>),
        sequence: Option<u32>,
    ) -> TemporalCoordinate {
        let mut explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension(kind)
            .with_scope_id(scope);
        if let Some(at) = occurred_at {
            explanation = explanation.with_occurred_at(at);
        }
        if let Some(from) = validity.0 {
            explanation = explanation.with_valid_from(from);
        }
        if let Some(until) = validity.1 {
            explanation = explanation.with_valid_until(until);
        }
        if let Some(sequence) = sequence {
            explanation = explanation.with_sequence(sequence);
        }
        TemporalCoordinate::from_relation_explanation(&explanation)
            .expect("coordinate")
            .expect("placed")
    }

    fn fact(
        ref_id: &str,
        about: &str,
        coordinates: Vec<TemporalCoordinate>,
        state: FactState,
    ) -> RelatedFact {
        RelatedFact::new(ref_id, about, coordinates, state).expect("fact")
    }

    #[test]
    fn the_same_scope_under_two_kinds_is_two_labels_and_relates_nothing() {
        let facts = vec![
            fact(
                "project:alpha:e1",
                "project:alpha",
                vec![coordinate_of_kind(
                    "owner",
                    "about:project:alpha:dimension:kmp",
                    Some("2026-03-04T01:00:00Z"),
                    (None, None),
                    Some(1),
                )],
                FactState::Current,
            ),
            fact(
                "project:beta:e1",
                "project:beta",
                vec![coordinate_of_kind(
                    "repo",
                    "about:project:beta:dimension:kmp",
                    Some("2026-03-04T01:20:00Z"),
                    (None, None),
                    Some(1),
                )],
                FactState::Current,
            ),
        ];

        let relations = relate(&facts, &[], TemporalAxis::Default);

        assert!(
            relations.coordinate.is_empty(),
            "`owner=kmp` and `repo=kmp` share a value, not a label: {:?}",
            relations.coordinate
        );
    }

    #[test]
    fn a_coordinate_relation_names_the_label_it_was_read_in() {
        let facts = vec![
            fact(
                "project:alpha:e1",
                "project:alpha",
                vec![coordinate(
                    "about:project:alpha:dimension:north",
                    Some("2026-03-04T01:00:00Z"),
                    (None, None),
                    None,
                )],
                FactState::Current,
            ),
            fact(
                "project:beta:e1",
                "project:beta",
                vec![coordinate(
                    "about:project:beta:dimension:north",
                    Some("2026-03-04T01:20:00Z"),
                    (None, None),
                    None,
                )],
                FactState::Current,
            ),
        ];

        let relations = relate(&facts, &[], TemporalAxis::Default);

        assert_eq!(relations.coordinate.len(), 1);
        assert_eq!(relations.coordinate[0].dimension(), "incident");
        assert_eq!(relations.coordinate[0].scope_id(), "north");
        assert!(
            relations.coordinate[0]
                .why()
                .starts_with("Both stand in `incident=north`;"),
            "{}",
            relations.coordinate[0].why()
        );
    }

    #[test]
    fn facts_of_different_abouts_in_one_scope_are_ordered_on_the_clock_read() {
        let facts = vec![
            fact(
                "project:alpha:e1",
                "project:alpha",
                vec![coordinate(
                    "about:project:alpha:dimension:incident:north",
                    Some("2026-03-04T01:00:00Z"),
                    (None, None),
                    Some(1),
                )],
                FactState::Current,
            ),
            fact(
                "project:beta:e1",
                "project:beta",
                vec![coordinate(
                    "about:project:beta:dimension:incident:north",
                    Some("2026-03-04T01:20:00Z"),
                    (None, None),
                    Some(1),
                )],
                FactState::Current,
            ),
            fact(
                "project:alpha:e2",
                "project:alpha",
                vec![coordinate(
                    "about:project:alpha:dimension:incident:north",
                    Some("2026-03-04T02:00:00Z"),
                    (None, None),
                    None,
                )],
                FactState::Current,
            ),
            fact(
                "project:gamma:e1",
                "project:gamma",
                vec![coordinate(
                    "about:project:gamma:dimension:work:main",
                    Some("2026-03-04T02:00:00Z"),
                    (None, None),
                    None,
                )],
                FactState::Current,
            ),
        ];
        let relations = relate(&facts, &[], TemporalAxis::Default);
        let kinds = relations
            .coordinate
            .iter()
            .map(|relation| {
                (
                    relation.from().to_string(),
                    relation.to().to_string(),
                    relation.kind(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                (
                    "project:alpha:e1".to_string(),
                    "project:beta:e1".to_string(),
                    CoordinateRelationKind::Before
                ),
                (
                    "project:alpha:e1".to_string(),
                    "project:beta:e1".to_string(),
                    CoordinateRelationKind::SameSequence
                ),
                (
                    "project:alpha:e2".to_string(),
                    "project:beta:e1".to_string(),
                    CoordinateRelationKind::After
                ),
            ],
            "two facts of one about never relate to each other here, and gamma shares no scope"
        );
        assert_eq!(relations.coordinate[0].scope_id(), "incident:north");
        assert!(relations.coordinate[0].why().contains("comes before"));
        assert_eq!(relations.omitted_coordinate, 0);
    }

    #[test]
    fn validity_spans_nest_touch_or_stand_apart() {
        let alpha = |from: &str, until: Option<&str>| {
            fact(
                "project:alpha:rule",
                "project:alpha",
                vec![coordinate(
                    "about:project:alpha:dimension:release:spring",
                    None,
                    (Some(from), until),
                    None,
                )],
                FactState::Current,
            )
        };
        let beta = |from: &str, until: Option<&str>| {
            fact(
                "project:beta:rule",
                "project:beta",
                vec![coordinate(
                    "about:project:beta:dimension:release:spring",
                    None,
                    (Some(from), until),
                    None,
                )],
                FactState::Current,
            )
        };
        let kind = |facts: Vec<RelatedFact>| {
            let relations = relate(&facts, &[], TemporalAxis::Validity);
            (
                relations.coordinate[0].from().to_string(),
                relations.coordinate[0].kind(),
            )
        };
        assert_eq!(
            kind(vec![
                alpha("2026-03-10T00:00:00Z", Some("2026-03-12T00:00:00Z")),
                beta("2026-03-01T00:00:00Z", Some("2026-04-01T00:00:00Z"))
            ]),
            (
                "project:alpha:rule".to_string(),
                CoordinateRelationKind::During
            )
        );
        assert_eq!(
            kind(vec![
                alpha("2026-03-01T00:00:00Z", Some("2026-04-01T00:00:00Z")),
                beta("2026-03-10T00:00:00Z", Some("2026-03-12T00:00:00Z"))
            ]),
            (
                "project:beta:rule".to_string(),
                CoordinateRelationKind::During
            ),
            "the inner span is always the one that holds `during` the other"
        );
        assert_eq!(
            kind(vec![
                alpha("2026-03-01T00:00:00Z", Some("2026-03-15T00:00:00Z")),
                beta("2026-03-10T00:00:00Z", None)
            ])
            .1,
            CoordinateRelationKind::Concurrent
        );
        assert_eq!(
            kind(vec![
                alpha("2026-03-01T00:00:00Z", Some("2026-03-10T00:00:00Z")),
                beta("2026-03-10T00:00:00Z", None)
            ])
            .1,
            CoordinateRelationKind::Before,
            "an exclusive end that meets the next start stands apart"
        );
    }

    #[test]
    fn a_fact_without_the_clock_read_only_shares_the_scope() {
        let facts = vec![
            fact(
                "project:alpha:e1",
                "project:alpha",
                vec![coordinate(
                    "about:project:alpha:dimension:incident:north",
                    Some("2026-03-04T01:00:00Z"),
                    (None, None),
                    None,
                )],
                FactState::Current,
            ),
            fact(
                "project:beta:e1",
                "project:beta",
                vec![coordinate(
                    "about:project:beta:dimension:incident:north",
                    None,
                    (None, None),
                    None,
                )],
                FactState::Current,
            ),
        ];
        let relations = relate(&facts, &[], TemporalAxis::Occurred);
        assert_eq!(relations.coordinate.len(), 1);
        assert_eq!(
            relations.coordinate[0].kind(),
            CoordinateRelationKind::SharesScope
        );
        assert!(
            relations.coordinate[0]
                .why()
                .contains("neither carries an instant")
        );
    }

    #[test]
    fn a_declared_contradiction_between_two_standing_facts_is_a_tension() {
        let facts = vec![
            fact(
                "project:alpha:freeze",
                "project:alpha",
                vec![coordinate(
                    "about:project:alpha:dimension:release:spring",
                    Some("2026-03-01T00:00:00Z"),
                    (None, None),
                    None,
                )],
                FactState::Current,
            ),
            fact(
                "project:alpha:ship",
                "project:alpha",
                vec![coordinate(
                    "about:project:alpha:dimension:release:spring",
                    Some("2026-03-05T00:00:00Z"),
                    (None, None),
                    None,
                )],
                FactState::Current,
            ),
            fact(
                "project:alpha:old",
                "project:alpha",
                vec![coordinate(
                    "about:project:alpha:dimension:release:spring",
                    Some("2026-02-01T00:00:00Z"),
                    (None, None),
                    None,
                )],
                FactState::Superseded,
            ),
        ];
        let declared = vec![
            DeclaredEdge {
                from: "project:alpha:ship".into(),
                to: "project:alpha:freeze".into(),
                rel: "contradicts".into(),
                why: "Shipping during a freeze.".into(),
                evidence: "Release calendar.".into(),
            },
            DeclaredEdge {
                from: "project:alpha:ship".into(),
                to: "project:alpha:old".into(),
                rel: "contradicts".into(),
                why: "".into(),
                evidence: "".into(),
            },
            DeclaredEdge {
                from: "project:alpha:ship".into(),
                to: "project:alpha:freeze".into(),
                rel: "follows".into(),
                why: "".into(),
                evidence: "".into(),
            },
        ];
        let relations = relate(&facts, &declared, TemporalAxis::Default);
        assert_eq!(
            relations.tensions.len(),
            1,
            "a replaced fact is no longer in tension, and `follows` is no contradiction"
        );
        let tension = &relations.tensions[0];
        assert_eq!(tension.ref_id(), "project:alpha:ship");
        assert_eq!(tension.other(), "project:alpha:freeze");
        assert_eq!(tension.scope_id(), "release:spring");
        assert_eq!(tension.why(), "Shipping during a freeze.");
        assert!(
            relations.coordinate.is_empty(),
            "one about relates to nothing by coordinate"
        );
    }

    #[test]
    fn the_cap_counts_what_it_leaves_out() {
        let mut facts = Vec::new();
        for index in 0..40 {
            for about in ["project:alpha", "project:beta"] {
                facts.push(fact(
                    &format!("{about}:e{index}"),
                    about,
                    vec![coordinate(
                        &format!("about:{about}:dimension:work:main"),
                        Some("2026-03-04T01:00:00Z"),
                        (None, None),
                        None,
                    )],
                    FactState::Current,
                ));
            }
        }
        let relations = relate(&facts, &[], TemporalAxis::Default);
        assert_eq!(relations.coordinate.len(), MAX_COORDINATE_RELATIONS);
        assert_eq!(
            relations.omitted_coordinate,
            40 * 40 - MAX_COORDINATE_RELATIONS
        );
    }
}
