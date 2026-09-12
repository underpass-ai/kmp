//! Rendering a proof and its parts as JSON: the path, the evidence bodies,
//! the supersessions and expiries, and the clocks that place them in time.

use kmp_proto::v1beta1::{
    ExpiredMemory, MemoryEvidence, MemoryRelation, SupersededMemory, TemporalCoordinate,
    TemporalCursor,
};
use serde_json::{Map, Value, json};

use super::normalization::normalized_proof_relation;
use super::scalars::{confidence_label, insert_non_empty, insert_timestamp, semantic_class_label};

pub(super) fn proof_value(proof: &kmp_proto::v1beta1::Proof, normalize: bool) -> Value {
    json!({
        "path": proof.path.iter().map(|relation| if normalize { memory_relation_value(&normalized_proof_relation(relation, &proof.evidence)) } else { memory_relation_value(relation) }).collect::<Vec<_>>(),
        "evidence": proof.evidence.iter().map(memory_evidence_value).collect::<Vec<_>>(),
        "conflicts": proof.conflicts,
        "superseded": proof.superseded.iter().map(superseded_value).collect::<Vec<_>>(),
        "expired": proof.expired.iter().map(expired_value).collect::<Vec<_>>(),
        "missing": proof.missing,
        "frontier_size": proof.frontier_size,
        "matched_terms": proof.matched_terms,
        "matched_relations": proof.matched_relations,
        "confidence": confidence_label(proof.confidence),
        "interval": proof.interval.as_ref().map(interval_value),
        "axis": (proof.interval.is_some() || proof.as_of.is_some()).then(|| temporal_axis_label(proof.axis)),
        "as_of": proof.as_of.map(|at| at.to_string()),
        "nearest_outside": proof.nearest_outside.as_ref().map(nearest_outside_value),
        "abouts_selected": proof.abouts_selected,
        "abouts_empty_in_selection": proof.abouts_empty_in_selection
    })
}

/// Where a recall stood in time, as the proof declares it: the keys are
/// present and null when the caller named no instant and no span.
fn interval_value(interval: &kmp_proto::v1beta1::TemporalInterval) -> Value {
    json!({
        "start": interval.start.map(|at| at.to_string()),
        "end": interval.end.map(|at| at.to_string())
    })
}

fn nearest_outside_value(nearest: &kmp_proto::v1beta1::NearestOutside) -> Value {
    json!({
        "ref": nearest.r#ref,
        "time": nearest.time.map(|at| at.to_string()),
        "axis": temporal_axis_label(nearest.axis)
    })
}

pub(super) fn temporal_axis_label(value: i32) -> &'static str {
    match kmp_proto::v1beta1::TemporalAxis::try_from(value) {
        Ok(kmp_proto::v1beta1::TemporalAxis::Occurred) => "occurred",
        Ok(kmp_proto::v1beta1::TemporalAxis::Observed) => "observed",
        Ok(kmp_proto::v1beta1::TemporalAxis::Ingested) => "ingested",
        Ok(kmp_proto::v1beta1::TemporalAxis::Validity) => "validity",
        _ => "default",
    }
}

pub(super) fn empty_proof_value() -> Value {
    json!({
        "path": [], "evidence": [], "conflicts": [], "superseded": [], "expired": [],
        "missing": ["proof"], "frontier_size": 1, "matched_terms": [],
        "matched_relations": [], "confidence": "unknown",
        "interval": null, "axis": null, "as_of": null, "nearest_outside": null,
        "abouts_selected": [], "abouts_empty_in_selection": []
    })
}

fn superseded_value(entry: &SupersededMemory) -> Value {
    let mut value = Map::new();
    value.insert("ref".to_string(), json!(entry.r#ref));
    value.insert("superseded_by".to_string(), json!(entry.superseded_by));
    insert_non_empty(&mut value, "why", &entry.why);
    Value::Object(value)
}

pub(super) fn expired_value(entry: &ExpiredMemory) -> Value {
    let mut value = Map::new();
    value.insert("ref".to_string(), json!(entry.r#ref));
    insert_timestamp(&mut value, "valid_until", entry.valid_until);
    Value::Object(value)
}

pub(super) fn memory_relation_value(relation: &MemoryRelation) -> Value {
    let mut value = Map::new();
    value.insert("from".to_string(), json!(relation.source_ref));
    value.insert("to".to_string(), json!(relation.target_ref));
    value.insert("rel".to_string(), json!(relation.rel));
    value.insert(
        "class".to_string(),
        json!(semantic_class_label(relation.semantic_class)),
    );
    insert_non_empty(&mut value, "why", &relation.why);
    insert_non_empty(&mut value, "evidence", &relation.evidence);
    value.insert(
        "confidence".to_string(),
        json!(confidence_label(relation.confidence)),
    );
    if let Some(sequence) = relation.sequence {
        value.insert("sequence".to_string(), json!(sequence));
    }
    if let Some(explanation) = relation.explanation.as_ref() {
        insert_non_empty(&mut value, "motivation", &explanation.motivation);
        insert_non_empty(&mut value, "method", &explanation.method);
        insert_non_empty(&mut value, "decision_id", &explanation.decision_id);
        insert_non_empty(
            &mut value,
            "caused_by_node_id",
            &explanation.caused_by_node_id,
        );
        if let Some(clocks) = explanation.clocks.as_ref() {
            let mut times = Map::new();
            insert_timestamp(&mut times, "occurred_at", clocks.occurred_at);
            insert_timestamp(&mut times, "observed_at", clocks.observed_at);
            insert_timestamp(&mut times, "ingested_at", clocks.ingested_at);
            insert_timestamp(&mut times, "valid_from", clocks.valid_from);
            insert_timestamp(&mut times, "valid_until", clocks.valid_until);
            value.insert("clocks".to_string(), Value::Object(times));
        }
        if let Some(coordinate) = explanation.coordinate.as_ref() {
            value.insert(
                "coordinate".to_string(),
                temporal_coordinate_value(coordinate),
            );
        }
    }
    if !relation.evidence_refs.is_empty() {
        value.insert("evidence_refs".to_string(), json!(relation.evidence_refs));
    }
    Value::Object(value)
}

pub(super) fn memory_evidence_value(evidence: &MemoryEvidence) -> Value {
    let mut value = Map::new();
    value.insert("id".to_string(), json!(evidence.id));
    value.insert("supports".to_string(), json!(evidence.supports));
    value.insert("text".to_string(), json!(evidence.text));
    insert_non_empty(&mut value, "source", &evidence.source);
    insert_timestamp(&mut value, "time", evidence.time);
    if let Some(clocks) = &evidence.support_clocks {
        let mut times = Map::new();
        insert_timestamp(&mut times, "observed_at", clocks.observed_at);
        insert_timestamp(&mut times, "ingested_at", clocks.ingested_at);
        value.insert("support_clocks".to_string(), Value::Object(times));
    }
    if !evidence.metadata.is_empty() {
        value.insert("metadata".to_string(), json!(evidence.metadata));
    }
    Value::Object(value)
}

pub(super) fn temporal_cursor_value(cursor: &TemporalCursor) -> Value {
    let mut value = Map::new();
    insert_non_empty(&mut value, "ref", &cursor.r#ref);
    insert_timestamp(&mut value, "time", cursor.time);
    if let Some(sequence) = cursor.sequence {
        value.insert("sequence".to_string(), json!(sequence));
    }
    Value::Object(value)
}

fn temporal_coordinate_value(coordinate: &TemporalCoordinate) -> Value {
    let mut value = Map::new();
    insert_non_empty(&mut value, "dimension", &coordinate.dimension);
    insert_non_empty(&mut value, "scope_id", &coordinate.scope_id);
    insert_timestamp(&mut value, "occurred_at", coordinate.occurred_at);
    insert_timestamp(&mut value, "observed_at", coordinate.observed_at);
    insert_timestamp(&mut value, "ingested_at", coordinate.ingested_at);
    insert_timestamp(&mut value, "valid_from", coordinate.valid_from);
    insert_timestamp(&mut value, "valid_until", coordinate.valid_until);
    if let Some(sequence) = coordinate.sequence {
        value.insert("sequence".to_string(), json!(sequence));
    }
    if let Some(rank) = coordinate.rank {
        value.insert("rank".to_string(), json!(rank));
    }
    if !coordinate.metadata.is_empty() {
        value.insert("metadata".to_string(), json!(coordinate.metadata));
    }
    Value::Object(value)
}
