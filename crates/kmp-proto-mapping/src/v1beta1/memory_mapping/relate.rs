//! `relate` on the wire: the neighbourhood the application loaded, read
//! into facts inside the selection, the relations each about declared
//! between them, what facts of different abouts have to do with each
//! other, and the tensions that still stand — with a proof that says where
//! the read stood, one page at a time.

use std::collections::{BTreeMap, BTreeSet};

use kmp_application::{GetContextResult, RelateMemoryQuery};
use kmp_domain::{
    DeclaredEdge, FactState, KmpBundle, MemoryRelationType, RelatedFact, TemporalAxis,
    TemporalCoordinate, relate,
};
use kmp_proto::v1beta1::{
    CoordinateRelation as ProtoCoordinateRelation, CoordinateRelationKind as ProtoKind,
    FactState as ProtoFactState, MemoryConfidence, MemoryEvidence, MemoryRelation, PageInfo, Proof,
    RelateResponse, RelatedFact as ProtoRelatedFact, SupersededMemory, Tension as ProtoTension,
};

use super::bundle_views::{
    about_by_entry, abouts_in_bundle, bundle_node_properties, memory_relations_from_bundle,
    persisted_memory_metadata, proto_coordinate_from_domain,
};
use super::memory_lifecycle::MemoryLifecycle;
use super::scalars::{ProtoMappingResult, proto_temporal_axis};
use super::temporal_admission::TemporalAdmission;

pub fn relate_response_from_result(
    result: GetContextResult,
    query: &RelateMemoryQuery,
) -> ProtoMappingResult<RelateResponse> {
    let bundle = &result.bundle;
    let admission = TemporalAdmission::read(bundle, &query.temporal)?;
    let bounded = admission.bound(bundle);
    let lifecycle = match admission.lifecycle_instant() {
        Some(instant) => MemoryLifecycle::read_at(&bounded, instant, admission.axis()),
        None => MemoryLifecycle::read(&bounded),
    };
    let axis = admission.axis();
    let owners = about_by_entry(bundle);
    let abouts = abouts_in_bundle(bundle);

    // Every entry the bundle places in time, split by the selection.
    let mut coordinates_by_ref = BTreeMap::<String, Vec<TemporalCoordinate>>::new();
    for relationship in bundle
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "contains_entry")
    {
        if let Ok(Some(coordinate)) =
            TemporalCoordinate::from_relation_explanation(relationship.explanation())
        {
            coordinates_by_ref
                .entry(relationship.target_node_id().to_string())
                .or_default()
                .push(coordinate);
        }
    }
    let (inside, outside): (Vec<_>, Vec<_>) = coordinates_by_ref
        .keys()
        .cloned()
        .partition(|entry_ref| admission.admits_entry(entry_ref));

    let superseded_by = bounded
        .relationships()
        .iter()
        .filter(|relationship| relationship.relationship_type() == "supersedes")
        .map(|relationship| {
            (
                relationship.target_node_id().to_string(),
                (
                    relationship.source_node_id().to_string(),
                    relationship
                        .explanation()
                        .rationale()
                        .unwrap_or_default()
                        .to_string(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let expired_until = lifecycle
        .expired_memories()
        .into_iter()
        .map(|expired| (expired.r#ref, expired.valid_until))
        .collect::<BTreeMap<_, _>>();

    let mut domain_facts = Vec::new();
    let mut facts = Vec::new();
    for entry_ref in &inside {
        let coordinates = coordinates_by_ref.remove(entry_ref).unwrap_or_default();
        let about = owners.get(entry_ref).cloned().unwrap_or_default();
        let state = if lifecycle.is_superseded(entry_ref) {
            FactState::Superseded
        } else if lifecycle.is_expired(entry_ref) {
            FactState::Expired
        } else {
            FactState::Current
        };
        let properties = bundle_node_properties(bundle, entry_ref);
        let text = entry_text(bundle, entry_ref);
        facts.push(ProtoRelatedFact {
            r#ref: entry_ref.clone(),
            about: about.clone(),
            kind: properties
                .and_then(|properties| properties.get("entry_kind"))
                .cloned()
                .unwrap_or_else(|| "entry".to_string()),
            text,
            coordinates: coordinates
                .iter()
                .map(proto_coordinate_from_domain)
                .collect(),
            state: proto_state(state) as i32,
            superseded_by: superseded_by
                .get(entry_ref)
                .map(|(by, _)| by.clone())
                .unwrap_or_default(),
            valid_until: expired_until.get(entry_ref).copied().flatten(),
            metadata: properties
                .map(persisted_memory_metadata)
                .unwrap_or_default(),
        });
        if let Ok(fact) = RelatedFact::new(entry_ref.clone(), about, coordinates, state) {
            domain_facts.push(fact);
        }
    }
    let fact_refs = inside.iter().cloned().collect::<BTreeSet<_>>();

    // What each about declared between its own facts, the structure left out.
    let declared = memory_relations_from_bundle(&bounded)
        .into_iter()
        .filter(|relation| {
            fact_refs.contains(&relation.source_ref)
                && fact_refs.contains(&relation.target_ref)
                && MemoryRelationType::new(&relation.rel)
                    .is_ok_and(|relation_type| !relation_type.is_structural())
        })
        .collect::<Vec<MemoryRelation>>();
    let edges = declared
        .iter()
        .map(|relation| DeclaredEdge {
            from: relation.source_ref.clone(),
            to: relation.target_ref.clone(),
            rel: relation.rel.clone(),
            why: relation.why.clone(),
            evidence: relation.evidence.clone(),
        })
        .collect::<Vec<_>>();

    let relations = relate(&domain_facts, &edges, axis);
    let coordinate = relations
        .coordinate
        .iter()
        .map(|relation| ProtoCoordinateRelation {
            from: relation.from().to_string(),
            to: relation.to().to_string(),
            kind: proto_kind(relation.kind()) as i32,
            scope_id: relation.scope_id().to_string(),
            axis: proto_temporal_axis(relation.axis()) as i32,
            why: relation.why(),
        })
        .collect::<Vec<_>>();
    let tensions = relations
        .tensions
        .iter()
        .map(|tension| ProtoTension {
            r#ref: tension.ref_id().to_string(),
            other: tension.other().to_string(),
            scope_id: tension.scope_id().to_string(),
            why: tension.why().to_string(),
            evidence: tension.evidence().to_string(),
        })
        .collect::<Vec<_>>();

    // One ordered sequence — facts, declared, coordinate, tensions — paged
    // by position, so a continuation never repeats and never skips.
    let total = facts.len() + declared.len() + coordinate.len() + tensions.len();
    let offset = query.page.offset().min(total);
    let end = offset
        .saturating_add(query.page.entries_or_default())
        .min(total);
    let has_more = end < total;
    let facts_page = slice_section(&facts, 0, offset, end);
    let declared_page = slice_section(&declared, facts.len(), offset, end);
    let coordinate_page = slice_section(&coordinate, facts.len() + declared.len(), offset, end);
    let tensions_page = slice_section(
        &tensions,
        facts.len() + declared.len() + coordinate.len(),
        offset,
        end,
    );

    let mut warnings = Vec::new();
    if offset >= total && total > 0 {
        warnings.push(format!(
            "relate page cursor {offset} is at or beyond the {total} items of this reading"
        ));
    }
    if has_more {
        warnings.push("relate response paginated; use page.next_cursor to continue".to_string());
    }
    if relations.omitted_coordinate > 0 {
        warnings.push(format!(
            "{} coordinate relations past the cap of {} were counted and not returned; narrow the interval or the abouts",
            relations.omitted_coordinate,
            kmp_domain::MAX_COORDINATE_RELATIONS
        ));
    }

    let mut proof = Proof {
        confidence: MemoryConfidence::Unspecified as i32,
        superseded: facts
            .iter()
            .filter_map(|fact| {
                let (by, why) = superseded_by.get(&fact.r#ref)?;
                Some(SupersededMemory {
                    r#ref: fact.r#ref.clone(),
                    superseded_by: by.clone(),
                    why: why.clone(),
                })
            })
            .collect(),
        expired: lifecycle.expired_memories(),
        ..Proof::default()
    };
    let stood = admission.proof_fields();
    proof.interval = stood.interval;
    proof.axis = stood.axis;
    proof.as_of = stood.as_of;
    proof.abouts_empty_in_selection = admission.abouts_empty_in_selection(&abouts, &owners);
    proof.abouts_selected = abouts.clone();
    if facts.is_empty() {
        proof
            .missing
            .push("any fact of the selected abouts within the selection".to_string());
        proof.frontier_size = 1;
        if admission.bounds_a_span() {
            let candidates = outside
                .iter()
                .map(|entry_ref| MemoryEvidence {
                    id: format!("entry:{entry_ref}"),
                    supports: vec![entry_ref.clone()],
                    ..MemoryEvidence::default()
                })
                .collect::<Vec<_>>();
            proof.nearest_outside = admission.nearest_outside(&candidates);
        }
    }

    let summary = if facts.is_empty() {
        match &proof.nearest_outside {
            Some(nearest) => format!(
                "No fact of {} falls within the selection; the nearest outside it is {} at {}",
                abouts_phrase(&abouts),
                nearest.r#ref,
                nearest
                    .time
                    .map(|time| time.to_string())
                    .unwrap_or_else(|| "an unread instant".to_string())
            ),
            None => format!(
                "No fact of {} falls within the selection",
                abouts_phrase(&abouts)
            ),
        }
    } else {
        format!(
            "Related {} {} of {} on the {} clock: {} declared, {} by coordinate, {} in tension",
            facts.len(),
            if facts.len() == 1 { "fact" } else { "facts" },
            abouts_phrase(&abouts),
            axis_name(axis),
            declared.len(),
            coordinate.len(),
            tensions.len()
        )
    };

    Ok(RelateResponse {
        summary,
        facts: facts_page,
        declared: declared_page,
        coordinate: coordinate_page,
        tensions: tensions_page,
        proof: Some(proof),
        warnings,
        page: Some(PageInfo {
            returned: u32::try_from(end.saturating_sub(offset)).unwrap_or(u32::MAX),
            total: u32::try_from(total).unwrap_or(u32::MAX),
            has_more,
            next_cursor: if has_more {
                end.to_string()
            } else {
                String::new()
            },
        }),
    })
}

/// The items of one section that fall in `[offset, end)` of the flattened
/// sequence, given where the section starts in it.
fn slice_section<T: Clone>(section: &[T], start: usize, offset: usize, end: usize) -> Vec<T> {
    let from = offset.saturating_sub(start).min(section.len());
    let to = end.saturating_sub(start).min(section.len());
    if from >= to {
        return Vec::new();
    }
    section[from..to].to_vec()
}

fn entry_text(bundle: &KmpBundle, entry_ref: &str) -> String {
    std::iter::once(bundle.root_node())
        .chain(bundle.neighbor_nodes())
        .find(|node| node.node_id() == entry_ref)
        .map(|node| node.summary().to_string())
        .unwrap_or_default()
}

fn abouts_phrase(abouts: &[String]) -> String {
    match abouts.len() {
        0 => "no about".to_string(),
        1 => format!("`{}`", abouts[0]),
        count => format!("{count} abouts"),
    }
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

fn proto_state(state: FactState) -> ProtoFactState {
    match state {
        FactState::Current => ProtoFactState::Current,
        FactState::Superseded => ProtoFactState::Superseded,
        FactState::Expired => ProtoFactState::Expired,
    }
}

fn proto_kind(kind: kmp_domain::CoordinateRelationKind) -> ProtoKind {
    use kmp_domain::CoordinateRelationKind as Kind;
    match kind {
        Kind::SharesScope => ProtoKind::SharesScope,
        Kind::Before => ProtoKind::Before,
        Kind::After => ProtoKind::After,
        Kind::During => ProtoKind::During,
        Kind::Concurrent => ProtoKind::Concurrent,
        Kind::SameSequence => ProtoKind::SameSequence,
        Kind::SameRank => ProtoKind::SameRank,
    }
}

#[cfg(test)]
mod tests {
    use super::slice_section;

    /// One flattened sequence of four sections, paged by position: a page
    /// takes what falls in its window from each section and nothing twice.
    #[test]
    fn a_page_window_cuts_each_section_by_its_position_in_the_sequence() {
        let facts = vec!["f1", "f2", "f3"];
        let declared = vec!["d1"];
        let coordinate = vec!["c1", "c2"];
        // Window [2, 5): the third fact, the declared edge, the first
        // coordinate relation.
        assert_eq!(slice_section(&facts, 0, 2, 5), vec!["f3"]);
        assert_eq!(slice_section(&declared, 3, 2, 5), vec!["d1"]);
        assert_eq!(slice_section(&coordinate, 4, 2, 5), vec!["c1"]);
        // The next window [5, 6) picks up exactly where it stopped.
        assert_eq!(slice_section(&facts, 0, 5, 6), Vec::<&str>::new());
        assert_eq!(slice_section(&coordinate, 4, 5, 6), vec!["c2"]);
    }
}
