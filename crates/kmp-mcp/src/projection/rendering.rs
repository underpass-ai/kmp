//! Proto values as the JSON a caller reads.
//!
//! One concept: how a kernel value is written down. Every function here takes
//! a `kmp_proto` value and returns `serde_json::Value`, and none of them
//! decides *whether* a value belongs in an answer — that is the mapper's
//! call, and trimming it afterwards is the budget's.
//!
//! The enum labels carry fallbacks (`"unspecified"`, `"default"`,
//! `"current_about"`) that the domain value objects cannot express, which is
//! why they are written here rather than delegated. Several of these
//! functions are near-duplicates of ones in
//! `kmp_proto_mapping::v1beta1::recall_projection`; that crate is published,
//! so unifying them is a public-surface change and not this refactor's.

use kmp_proto::v1beta1::{
    DimensionScopeMode, DimensionSelection, DimensionSelectionMode, ExpiredMemory,
    MemoryConfidence, MemoryEvidence, MemoryRelation, MemorySemanticClass, RawMemoryRef,
    SupersededMemory, TemporalAxis, TemporalCoordinate, TemporalCursor, TemporalDirection,
    TemporalEntry,
};
use prost_types::Timestamp;
use serde_json::{Map, Value, json};

pub(super) fn dimension_coverage_json(dimension: &kmp_proto::v1beta1::DimensionCoverage) -> Value {
    json!({
        "dimension": dimension.dimension,
        "returned": dimension.returned,
        "present": dimension.present
    })
}

pub(super) fn response_quality_json(quality: &kmp_proto::v1beta1::ResponseQuality) -> Value {
    json!({
        "nodes": quality.nodes,
        "relationships": quality.relationships,
        "details": quality.details,
        "detail_coverage": quality.detail_coverage,
        "causal_density": quality.causal_density,
        "truncated": quality.truncated
    })
}

pub(super) fn optional_quality_json(
    quality: Option<&kmp_proto::v1beta1::ResponseQuality>,
) -> Value {
    quality.map(response_quality_json).unwrap_or(Value::Null)
}

pub(super) fn proof_json(proof: &kmp_proto::v1beta1::Proof) -> Value {
    json!({
        "path": proof
            .path
            .iter()
            .map(|relation| proof_relation_json(relation, &proof.evidence))
            .collect::<Vec<_>>(),
        "evidence": proof.evidence.iter().map(memory_evidence_json).collect::<Vec<_>>(),
        "conflicts": proof.conflicts,
        // Kept apart from conflicts: a supersession is a lifecycle, not a
        // disagreement, and the older entry is history rather than advice.
        "superseded": proof
            .superseded
            .iter()
            .map(superseded_json)
            .collect::<Vec<_>>(),
        // Expiry is independent of supersession: a lease can simply stop
        // holding without another memory replacing it.
        "expired": proof
            .expired
            .iter()
            .map(expired_json)
            .collect::<Vec<_>>(),
        "missing": proof.missing,
        "frontier_size": proof.frontier_size,
        "matched_terms": proof.matched_terms,
        "matched_relations": proof.matched_relations,
        "confidence": confidence_label(proof.confidence),
        // Where the recall stood in time, when the caller named it. A
        // temporal move stands at its cursor and leaves these null.
        "interval": proof.interval.as_ref().map(|interval| json!({
            "start": interval.start.map(|at| at.to_string()),
            "end": interval.end.map(|at| at.to_string())
        })),
        "axis": (proof.interval.is_some() || proof.as_of.is_some())
            .then(|| temporal_axis_label(proof.axis)),
        "as_of": proof.as_of.map(|at| at.to_string()),
        "nearest_outside": proof.nearest_outside.as_ref().map(|nearest| json!({
            "ref": nearest.r#ref,
            "time": nearest.time.map(|at| at.to_string()),
            "axis": temporal_axis_label(nearest.axis)
        }))
    })
}

pub(super) fn proof_relation_json(relation: &MemoryRelation, evidence: &[MemoryEvidence]) -> Value {
    let mut relation = relation.clone();
    let mut refs = relation
        .evidence_refs
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut repeated_why = false;
    let mut repeated_evidence = false;

    for item in evidence {
        let evidence_node_ref = item.id.strip_prefix("detail:").unwrap_or(&item.id);
        let incident = relation.source_ref == evidence_node_ref
            || relation.target_ref == evidence_node_ref
            || item.supports.iter().any(|supported_ref| {
                relation.source_ref == *supported_ref || relation.target_ref == *supported_ref
            });
        let why_matches = !relation.why.is_empty() && relation.why == item.text;
        let evidence_matches = !relation.evidence.is_empty() && relation.evidence == item.text;
        if incident || why_matches || evidence_matches {
            refs.insert(item.id.clone());
        }
        repeated_why |= why_matches;
        repeated_evidence |= evidence_matches;
    }

    if repeated_why {
        relation.why.clear();
    }
    if repeated_evidence {
        relation.evidence.clear();
    }
    relation.evidence_refs = refs.into_iter().collect();
    if relation.semantic_class != MemorySemanticClass::Structural as i32
        && relation.why.is_empty()
        && relation.evidence.is_empty()
        && !relation.evidence_refs.is_empty()
    {
        relation.why = "Supported by canonical evidence refs.".to_string();
    }
    memory_relation_json(&relation)
}

pub(super) fn superseded_json(entry: &SupersededMemory) -> Value {
    let mut object = Map::new();
    object.insert("ref".to_string(), json!(entry.r#ref));
    object.insert("superseded_by".to_string(), json!(entry.superseded_by));
    insert_optional_string(&mut object, "why", &entry.why);
    Value::Object(object)
}

pub(super) fn expired_json(entry: &ExpiredMemory) -> Value {
    let mut object = Map::new();
    object.insert("ref".to_string(), json!(entry.r#ref));
    insert_optional_timestamp(&mut object, "valid_until", entry.valid_until);
    Value::Object(object)
}

pub(super) fn empty_proof_json() -> Value {
    json!({
        "path": [],
        "evidence": [],
        "conflicts": [],
        "superseded": [],
        "expired": [],
        "missing": ["proof"],
        "frontier_size": 1,
        "matched_terms": [],
        "matched_relations": [],
        "confidence": "unknown",
        "interval": null,
        "axis": null,
        "as_of": null,
        "nearest_outside": null
    })
}

pub(super) fn page_info_json(page: &kmp_proto::v1beta1::PageInfo) -> Value {
    json!({
        "returned": page.returned,
        "total": page.total,
        "has_more": page.has_more,
        "next_cursor": if page.next_cursor.trim().is_empty() {
            Value::Null
        } else {
            Value::String(page.next_cursor.clone())
        }
    })
}

pub(super) fn empty_page_info_json() -> Value {
    json!({
        "returned": 0,
        "total": 0,
        "has_more": false,
        "next_cursor": Value::Null
    })
}

pub(super) fn temporal_state_json(state: &kmp_proto::v1beta1::TemporalState) -> Value {
    json!({
        "direction": temporal_direction_label(state.direction),
        "axis": temporal_axis_label(state.axis),
        "requested": state
            .requested
            .as_ref()
            .map(temporal_cursor_json)
            .unwrap_or(Value::Null),
        "resolved": state
            .resolved
            .as_ref()
            .map(temporal_coordinate_json)
            .unwrap_or(Value::Null)
    })
}

pub(super) fn temporal_entry_json(entry: &TemporalEntry) -> Value {
    json!({
        "ref": entry.r#ref,
        "kind": entry.kind,
        "text": entry.text,
        "coordinates": entry.coordinates.iter().map(temporal_coordinate_json).collect::<Vec<_>>(),
        "metadata": entry.metadata
    })
}

pub(super) fn temporal_cursor_json(cursor: &TemporalCursor) -> Value {
    let mut object = Map::new();
    insert_optional_string(&mut object, "ref", &cursor.r#ref);
    if let Some(time) = cursor.time {
        object.insert("time".to_string(), json!(time.to_string()));
    }
    if let Some(sequence) = cursor.sequence {
        object.insert("sequence".to_string(), json!(sequence));
    }
    Value::Object(object)
}

pub(super) fn temporal_coordinate_json(coordinate: &TemporalCoordinate) -> Value {
    let mut object = Map::new();
    insert_optional_string(&mut object, "dimension", &coordinate.dimension);
    insert_optional_string(&mut object, "scope_id", &coordinate.scope_id);
    insert_optional_timestamp(&mut object, "occurred_at", coordinate.occurred_at);
    insert_optional_timestamp(&mut object, "observed_at", coordinate.observed_at);
    insert_optional_timestamp(&mut object, "ingested_at", coordinate.ingested_at);
    insert_optional_timestamp(&mut object, "valid_from", coordinate.valid_from);
    insert_optional_timestamp(&mut object, "valid_until", coordinate.valid_until);
    if let Some(sequence) = coordinate.sequence {
        object.insert("sequence".to_string(), json!(sequence));
    }
    if let Some(rank) = coordinate.rank {
        object.insert("rank".to_string(), json!(rank));
    }
    if !coordinate.metadata.is_empty() {
        object.insert("metadata".to_string(), json!(coordinate.metadata));
    }
    Value::Object(object)
}

pub(super) fn dimension_selection_json(selection: &DimensionSelection) -> Value {
    let mut object = Map::new();
    object.insert(
        "mode".to_string(),
        json!(dimension_selection_mode_label(selection.mode)),
    );
    if !selection.include.is_empty() {
        object.insert("include".to_string(), json!(selection.include));
    }
    if !selection.exclude.is_empty() {
        object.insert("exclude".to_string(), json!(selection.exclude));
    }
    if !selection.scope_ids.is_empty() {
        object.insert("scope_ids".to_string(), json!(selection.scope_ids));
    }
    object.insert(
        "scope".to_string(),
        json!(dimension_scope_mode_label(selection.scope)),
    );
    if !selection.abouts.is_empty() {
        object.insert("abouts".to_string(), json!(selection.abouts));
    }
    Value::Object(object)
}

pub(super) fn memory_relation_json(relation: &MemoryRelation) -> Value {
    let mut object = Map::new();
    object.insert("from".to_string(), json!(relation.source_ref));
    object.insert("to".to_string(), json!(relation.target_ref));
    object.insert("rel".to_string(), json!(relation.rel));
    object.insert(
        "class".to_string(),
        json!(semantic_class_label(relation.semantic_class)),
    );
    insert_optional_string(&mut object, "why", &relation.why);
    insert_optional_string(&mut object, "evidence", &relation.evidence);
    object.insert(
        "confidence".to_string(),
        json!(confidence_label(relation.confidence)),
    );
    if let Some(sequence) = relation.sequence {
        object.insert("sequence".to_string(), json!(sequence));
    }
    if let Some(explanation) = relation.explanation.as_ref() {
        insert_optional_string(&mut object, "motivation", &explanation.motivation);
        insert_optional_string(&mut object, "method", &explanation.method);
        insert_optional_string(&mut object, "decision_id", &explanation.decision_id);
        insert_optional_string(
            &mut object,
            "caused_by_node_id",
            &explanation.caused_by_node_id,
        );
        if let Some(coordinate) = explanation.coordinate.as_ref() {
            object.insert(
                "coordinate".to_string(),
                temporal_coordinate_json(coordinate),
            );
        }
    }
    if !relation.evidence_refs.is_empty() {
        object.insert("evidence_refs".to_string(), json!(relation.evidence_refs));
    }
    Value::Object(object)
}

pub(super) fn memory_evidence_json(evidence: &MemoryEvidence) -> Value {
    let mut object = Map::new();
    object.insert("id".to_string(), json!(evidence.id));
    object.insert("supports".to_string(), json!(evidence.supports));
    object.insert("text".to_string(), json!(evidence.text));
    insert_optional_string(&mut object, "source", &evidence.source);
    insert_optional_timestamp(&mut object, "time", evidence.time);
    if !evidence.metadata.is_empty() {
        object.insert("metadata".to_string(), json!(evidence.metadata));
    }
    Value::Object(object)
}

pub(super) fn raw_memory_ref_json(raw: &RawMemoryRef) -> Value {
    json!({
        "ref": raw.r#ref,
        "kind": raw.kind,
        "text": raw.text,
        "coordinates": raw.coordinates.iter().map(temporal_coordinate_json).collect::<Vec<_>>(),
        "detail": raw.detail,
        "content_hash": raw.content_hash,
        "revision": raw.revision
    })
}

pub(super) fn insert_optional_string(object: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        object.insert(key.to_string(), json!(value));
    }
}

pub(super) fn insert_optional_timestamp(
    object: &mut Map<String, Value>,
    key: &str,
    value: Option<Timestamp>,
) {
    if let Some(value) = value {
        object.insert(key.to_string(), json!(value.to_string()));
    }
}

pub(super) fn semantic_class_label(value: i32) -> &'static str {
    match MemorySemanticClass::try_from(value) {
        Ok(MemorySemanticClass::Structural) => "structural",
        Ok(MemorySemanticClass::Causal) => "causal",
        Ok(MemorySemanticClass::Motivational) => "motivational",
        Ok(MemorySemanticClass::Procedural) => "procedural",
        Ok(MemorySemanticClass::Evidential) => "evidential",
        Ok(MemorySemanticClass::Constraint) => "constraint",
        _ => "unspecified",
    }
}

pub(super) fn confidence_label(value: i32) -> &'static str {
    match MemoryConfidence::try_from(value) {
        Ok(MemoryConfidence::High) => "high",
        Ok(MemoryConfidence::Medium) => "medium",
        Ok(MemoryConfidence::Low) => "low",
        Ok(MemoryConfidence::Unknown) => "unknown",
        _ => "unspecified",
    }
}

pub(super) fn temporal_direction_label(value: i32) -> &'static str {
    match TemporalDirection::try_from(value) {
        Ok(TemporalDirection::Goto) => "goto",
        Ok(TemporalDirection::Near) => "near",
        Ok(TemporalDirection::Rewind) => "rewind",
        Ok(TemporalDirection::Forward) => "forward",
        _ => "unspecified",
    }
}

pub(super) fn temporal_axis_label(value: i32) -> &'static str {
    match TemporalAxis::try_from(value) {
        Ok(TemporalAxis::Occurred) => "occurred",
        Ok(TemporalAxis::Observed) => "observed",
        Ok(TemporalAxis::Ingested) => "ingested",
        Ok(TemporalAxis::Validity) => "validity",
        _ => "default",
    }
}

pub(super) fn dimension_selection_mode_label(value: i32) -> &'static str {
    match DimensionSelectionMode::try_from(value) {
        Ok(DimensionSelectionMode::All) => "all",
        Ok(DimensionSelectionMode::Only) => "only",
        Ok(DimensionSelectionMode::Except) => "except",
        _ => "unspecified",
    }
}

pub(super) fn dimension_scope_mode_label(value: i32) -> &'static str {
    match DimensionScopeMode::try_from(value) {
        Ok(DimensionScopeMode::CurrentAbout) => "current_about",
        Ok(DimensionScopeMode::Abouts) => "abouts",
        Ok(DimensionScopeMode::AllAbouts) => "all_abouts",
        _ => "current_about",
    }
}
