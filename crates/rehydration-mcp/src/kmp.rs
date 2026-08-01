use prost_types::Timestamp;
use rehydration_proto::v1beta1::{
    AnswerReason, AskResponse, DimensionScopeMode, DimensionSelection, DimensionSelectionMode,
    IngestResponse, InspectResponse, MemoryConfidence, MemoryEvidence, MemoryRelation,
    MemorySemanticClass, RawMemoryRef, TemporalCoordinate, TemporalCursor, TemporalDirection,
    TemporalEntry, TemporalMoveResponse, TraceResponse, WakeClaim, WakeResponse,
};
use serde_json::{Map, Value, json};

use crate::ingest::KmpIngestPlan;

pub(crate) fn ingest_from_response(response: IngestResponse) -> Value {
    let memory = response.memory.as_ref();
    let accepted = memory.and_then(|memory| memory.accepted.as_ref());

    json!({
        "summary": response.summary,
        "memory": {
            "about": memory.map(|memory| memory.about.as_str()).unwrap_or(""),
            "memory_id": memory.map(|memory| memory.memory_id.as_str()).unwrap_or(""),
            "accepted": {
                "entries": accepted.map(|accepted| accepted.entries).unwrap_or_default(),
                "relations": accepted.map(|accepted| accepted.relations).unwrap_or_default(),
                "evidence": accepted.map(|accepted| accepted.evidence).unwrap_or_default()
            },
            "read_after_write_ready": memory
                .map(|memory| memory.read_after_write_ready)
                .unwrap_or(false)
        },
        "warnings": response.warnings
    })
}

pub(crate) fn dry_run_ingest_from_plan(plan: &KmpIngestPlan) -> Value {
    json!({
        "summary": format!(
            "Ingested {} {}, {} {}, and {} {} for {}.",
            plan.accepted.entries,
            plural(plan.accepted.entries, "entry", "entries"),
            plan.accepted.relations,
            plural(plan.accepted.relations, "relation", "relations"),
            plan.accepted.evidence,
            plural(plan.accepted.evidence, "evidence item", "evidence items"),
            plan.about
        ),
        "memory": {
            "about": plan.about,
            "memory_id": plan.memory_id,
            "accepted": {
                "entries": plan.accepted.entries,
                "relations": plan.accepted.relations,
                "evidence": plan.accepted.evidence
            },
            "read_after_write_ready": false
        },
        "warnings": [
            "dry_run=true; validated memory without sending a KernelMemoryService.Ingest call"
        ]
    })
}

pub(crate) fn wake_from_response(response: WakeResponse) -> Value {
    let wake = response.wake.as_ref();
    json!({
        "summary": response.summary,
        "wake": {
            "objective": wake.map(|wake| wake.objective.as_str()).unwrap_or(""),
            "current_state": wake
                .map(|wake| wake.current_state.clone())
                .unwrap_or_default(),
            "causal_spine": wake
                .map(|wake| wake.causal_spine.iter().map(wake_claim_json).collect::<Vec<_>>())
                .unwrap_or_default(),
            "open_loops": wake.map(|wake| wake.open_loops.clone()).unwrap_or_default(),
            "next_actions": wake.map(|wake| wake.next_actions.clone()).unwrap_or_default(),
            "guardrails": wake.map(|wake| wake.guardrails.clone()).unwrap_or_default()
        },
        "proof": response.proof.as_ref().map(proof_json).unwrap_or_else(empty_proof_json),
        "warnings": response.warnings
    })
}

pub(crate) fn ask_from_response(response: AskResponse) -> Value {
    json!({
        "summary": response.summary,
        "answer": if response.answer.trim().is_empty() {
            Value::Null
        } else {
            Value::String(response.answer)
        },
        "because": response.because.iter().map(answer_reason_json).collect::<Vec<_>>(),
        "proof": response.proof.as_ref().map(proof_json).unwrap_or_else(empty_proof_json),
        "warnings": response.warnings
    })
}

pub(crate) fn temporal_from_response(response: TemporalMoveResponse) -> Value {
    json!({
        "summary": response.summary,
        "temporal": response
            .temporal
            .as_ref()
            .map(temporal_state_json)
            .unwrap_or(Value::Null),
        "coverage": response
            .coverage
            .as_ref()
            .map(|coverage| {
                json!({
                    "requested": coverage
                        .requested
                        .as_ref()
                        .map(dimension_selection_json)
                        .unwrap_or(Value::Null),
                    "included": coverage.included,
                    "missing": coverage.missing,
                    "dimensions": coverage
                        .dimensions
                        .iter()
                        .map(dimension_coverage_json)
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_else(|| json!({
                "requested": Value::Null,
                "included": Vec::<String>::new(),
                "missing": Vec::<String>::new(),
                "dimensions": Vec::<Value>::new()
        })),
        "entries": response.entries.iter().map(temporal_entry_json).collect::<Vec<_>>(),
        "page": response
            .page
            .as_ref()
            .map(page_info_json)
            .unwrap_or_else(empty_page_info_json),
        "raw_refs": response.raw_refs.iter().map(raw_memory_ref_json).collect::<Vec<_>>(),
        "proof": response.proof.as_ref().map(proof_json).unwrap_or_else(empty_proof_json),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    })
}

pub(crate) fn trace_from_response(response: TraceResponse) -> Value {
    json!({
        "summary": response.summary,
        "trace": response.trace.iter().map(memory_relation_json).collect::<Vec<_>>(),
        "page": response
            .page
            .as_ref()
            .map(page_info_json)
            .unwrap_or_else(empty_page_info_json),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    })
}

pub(crate) fn inspect_from_response(response: InspectResponse) -> Value {
    let object = response.object.as_ref().map_or_else(
        || {
            json!({
                "ref": "",
                "kind": "",
                "text": "",
                "metadata": {}
            })
        },
        |object| {
            let mut value = Map::new();
            value.insert("ref".to_string(), json!(object.r#ref));
            value.insert("kind".to_string(), json!(object.kind));
            value.insert("text".to_string(), json!(object.text));
            value.insert("metadata".to_string(), json!(object.metadata));
            insert_optional_string(&mut value, "source", &object.source);
            Value::Object(value)
        },
    );
    let links = response.links.as_ref();

    json!({
        "summary": response.summary,
        "object": object,
        "links": {
            "incoming": links
                .map(|links| links.incoming.iter().map(memory_relation_json).collect::<Vec<_>>())
                .unwrap_or_default(),
            "outgoing": links
                .map(|links| links.outgoing.iter().map(memory_relation_json).collect::<Vec<_>>())
                .unwrap_or_default()
        },
        "evidence": response.evidence.iter().map(memory_evidence_json).collect::<Vec<_>>(),
        "raw": response.raw.iter().map(raw_memory_ref_json).collect::<Vec<_>>(),
        "quality": optional_quality_json(response.quality.as_ref()),
        "warnings": response.warnings
    })
}

fn wake_claim_json(claim: &WakeClaim) -> Value {
    json!({
        "claim": claim.claim,
        "because": claim.because,
        "evidence_ref": claim.evidence_ref
    })
}

fn answer_reason_json(reason: &AnswerReason) -> Value {
    json!({
        "claim": reason.claim,
        "evidence": reason.evidence,
        "ref": reason.r#ref
    })
}

fn dimension_coverage_json(dimension: &rehydration_proto::v1beta1::DimensionCoverage) -> Value {
    json!({
        "dimension": dimension.dimension,
        "returned": dimension.returned,
        "present": dimension.present
    })
}

fn response_quality_json(quality: &rehydration_proto::v1beta1::ResponseQuality) -> Value {
    json!({
        "nodes": quality.nodes,
        "relationships": quality.relationships,
        "details": quality.details,
        "detail_coverage": quality.detail_coverage,
        "causal_density": quality.causal_density,
        "truncated": quality.truncated
    })
}

fn optional_quality_json(quality: Option<&rehydration_proto::v1beta1::ResponseQuality>) -> Value {
    quality.map(response_quality_json).unwrap_or(Value::Null)
}

fn proof_json(proof: &rehydration_proto::v1beta1::Proof) -> Value {
    json!({
        "path": proof.path.iter().map(memory_relation_json).collect::<Vec<_>>(),
        "evidence": proof.evidence.iter().map(memory_evidence_json).collect::<Vec<_>>(),
        "conflicts": proof.conflicts,
        "missing": proof.missing,
        "frontier_size": proof.frontier_size,
        "confidence": confidence_label(proof.confidence)
    })
}

fn empty_proof_json() -> Value {
    json!({
        "path": [],
        "evidence": [],
        "conflicts": [],
        "missing": ["proof"],
        "frontier_size": 1,
        "confidence": "unknown"
    })
}

fn page_info_json(page: &rehydration_proto::v1beta1::PageInfo) -> Value {
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

fn empty_page_info_json() -> Value {
    json!({
        "returned": 0,
        "total": 0,
        "has_more": false,
        "next_cursor": Value::Null
    })
}

fn temporal_state_json(state: &rehydration_proto::v1beta1::TemporalState) -> Value {
    json!({
        "direction": temporal_direction_label(state.direction),
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

fn temporal_entry_json(entry: &TemporalEntry) -> Value {
    json!({
        "ref": entry.r#ref,
        "kind": entry.kind,
        "text": entry.text,
        "coordinates": entry.coordinates.iter().map(temporal_coordinate_json).collect::<Vec<_>>(),
        "metadata": entry.metadata
    })
}

fn temporal_cursor_json(cursor: &TemporalCursor) -> Value {
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

fn temporal_coordinate_json(coordinate: &TemporalCoordinate) -> Value {
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

fn dimension_selection_json(selection: &DimensionSelection) -> Value {
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

fn memory_relation_json(relation: &MemoryRelation) -> Value {
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
    Value::Object(object)
}

fn memory_evidence_json(evidence: &MemoryEvidence) -> Value {
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

fn raw_memory_ref_json(raw: &RawMemoryRef) -> Value {
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

fn insert_optional_string(object: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        object.insert(key.to_string(), json!(value));
    }
}

fn insert_optional_timestamp(object: &mut Map<String, Value>, key: &str, value: Option<Timestamp>) {
    if let Some(value) = value {
        object.insert(key.to_string(), json!(value.to_string()));
    }
}

fn semantic_class_label(value: i32) -> &'static str {
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

fn confidence_label(value: i32) -> &'static str {
    match MemoryConfidence::try_from(value) {
        Ok(MemoryConfidence::High) => "high",
        Ok(MemoryConfidence::Medium) => "medium",
        Ok(MemoryConfidence::Low) => "low",
        Ok(MemoryConfidence::Unknown) => "unknown",
        _ => "unspecified",
    }
}

fn temporal_direction_label(value: i32) -> &'static str {
    match TemporalDirection::try_from(value) {
        Ok(TemporalDirection::Goto) => "goto",
        Ok(TemporalDirection::Near) => "near",
        Ok(TemporalDirection::Rewind) => "rewind",
        Ok(TemporalDirection::Forward) => "forward",
        _ => "unspecified",
    }
}

fn dimension_selection_mode_label(value: i32) -> &'static str {
    match DimensionSelectionMode::try_from(value) {
        Ok(DimensionSelectionMode::All) => "all",
        Ok(DimensionSelectionMode::Only) => "only",
        Ok(DimensionSelectionMode::Except) => "except",
        _ => "unspecified",
    }
}

fn dimension_scope_mode_label(value: i32) -> &'static str {
    match DimensionScopeMode::try_from(value) {
        Ok(DimensionScopeMode::CurrentAbout) => "current_about",
        Ok(DimensionScopeMode::Abouts) => "abouts",
        Ok(DimensionScopeMode::AllAbouts) => "all_abouts",
        _ => "current_about",
    }
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

#[cfg(test)]
mod tests {
    use rehydration_proto::v1beta1::{
        MemoryBudget, MemoryDetailLevel, Proof, TemporalState, WakePacket,
    };

    use super::*;

    #[test]
    fn maps_typed_ask_response_without_inventing_null_answer() {
        let response = AskResponse {
            summary: "No deterministic memory answer found.".to_string(),
            answer: String::new(),
            because: vec![AnswerReason {
                claim: "claim".to_string(),
                evidence: "evidence".to_string(),
                r#ref: "evidence:1".to_string(),
            }],
            proof: Some(Proof {
                path: vec![relation()],
                evidence: vec![evidence()],
                conflicts: Vec::new(),
                missing: vec!["generative_answer".to_string()],
                frontier_size: 1,
                confidence: MemoryConfidence::Medium as i32,
            }),
            warnings: Vec::new(),
        };

        let value = ask_from_response(response);

        assert_eq!(value["answer"], Value::Null);
        assert_eq!(value["because"][0]["ref"], "evidence:1");
        assert_eq!(value["proof"]["path"][0]["from"], "claim:source");
        assert_eq!(value["proof"]["confidence"], "medium");
        assert_eq!(value["proof"]["frontier_size"], 1);
    }

    #[test]
    fn maps_temporal_response_to_kmp_json_names() {
        let response = TemporalMoveResponse {
            summary: "Returned 1 temporal entry.".to_string(),
            temporal: Some(TemporalState {
                direction: TemporalDirection::Forward as i32,
                requested: Some(TemporalCursor {
                    r#ref: "claim:source".to_string(),
                    time: None,
                    sequence: None,
                }),
                resolved: Some(coordinate()),
            }),
            coverage: Some(rehydration_proto::v1beta1::TemporalCoverage {
                requested: Some(DimensionSelection {
                    mode: DimensionSelectionMode::Only as i32,
                    include: vec!["timeline".to_string()],
                    exclude: Vec::new(),
                    scope: DimensionScopeMode::CurrentAbout as i32,
                    abouts: Vec::new(),
                    scope_ids: vec!["timeline:main".to_string()],
                }),
                included: vec!["timeline".to_string()],
                missing: Vec::new(),
                dimensions: vec![rehydration_proto::v1beta1::DimensionCoverage {
                    dimension: "timeline".to_string(),
                    returned: 1,
                    present: true,
                }],
            }),
            entries: vec![TemporalEntry {
                r#ref: "claim:target".to_string(),
                kind: "claim".to_string(),
                text: "Target".to_string(),
                coordinates: vec![coordinate()],
                metadata: [("window".to_string(), "10:00-10:20".to_string())]
                    .into_iter()
                    .collect(),
            }],
            proof: None,
            warnings: Vec::new(),
            raw_refs: Vec::new(),
            page: Some(rehydration_proto::v1beta1::PageInfo {
                returned: 1,
                total: 2,
                has_more: true,
                next_cursor: "claim:target".to_string(),
            }),
            quality: Some(rehydration_proto::v1beta1::ResponseQuality {
                nodes: 1,
                relationships: 0,
                details: 1,
                detail_coverage: 1.0,
                causal_density: 0.0,
                truncated: true,
            }),
        };

        let value = temporal_from_response(response);

        assert_eq!(value["temporal"]["direction"], "forward");
        assert_eq!(value["entries"][0]["ref"], "claim:target");
        assert_eq!(value["entries"][0]["coordinates"][0]["scope_id"], "scope");
        assert_eq!(value["entries"][0]["metadata"]["window"], "10:00-10:20");
        assert_eq!(value["coverage"]["requested"]["scope"], "current_about");
        assert_eq!(
            value["coverage"]["requested"]["scope_ids"][0],
            "timeline:main"
        );
        assert_eq!(value["coverage"]["dimensions"][0]["dimension"], "timeline");
        assert_eq!(value["coverage"]["dimensions"][0]["returned"], 1);
        assert_eq!(value["coverage"]["dimensions"][0]["present"], true);
        assert_eq!(value["quality"]["nodes"], 1);
        assert_eq!(value["quality"]["details"], 1);
        assert_eq!(value["quality"]["detail_coverage"], 1.0);
        assert_eq!(value["quality"]["truncated"], true);
        assert_eq!(value["page"]["returned"], 1);
        assert_eq!(value["page"]["total"], 2);
        assert_eq!(value["page"]["has_more"], true);
        assert_eq!(value["page"]["next_cursor"], "claim:target");
    }

    #[test]
    fn maps_wake_and_ignores_transport_budget_types() {
        let response = WakeResponse {
            summary: "Wake summary".to_string(),
            wake: Some(WakePacket {
                objective: "continue".to_string(),
                current_state: vec!["state".to_string()],
                causal_spine: vec![WakeClaim {
                    claim: "claim".to_string(),
                    because: "because".to_string(),
                    evidence_ref: "evidence:1".to_string(),
                }],
                open_loops: Vec::new(),
                next_actions: Vec::new(),
                guardrails: Vec::new(),
            }),
            proof: None,
            warnings: Vec::new(),
        };
        let _budget = MemoryBudget {
            tokens: 1,
            detail: MemoryDetailLevel::Full as i32,
            depth: 1,
            max_entries: 0,
        };

        let value = wake_from_response(response);

        assert_eq!(value["wake"]["current_state"][0], "state");
        assert_eq!(
            value["wake"]["causal_spine"][0]["evidence_ref"],
            "evidence:1"
        );
    }

    fn relation() -> MemoryRelation {
        MemoryRelation {
            source_ref: "claim:source".to_string(),
            target_ref: "claim:target".to_string(),
            rel: "supports".to_string(),
            semantic_class: MemorySemanticClass::Evidential as i32,
            why: "why".to_string(),
            evidence: "evidence".to_string(),
            confidence: MemoryConfidence::High as i32,
            sequence: Some(1),
            explanation: None,
        }
    }

    fn evidence() -> MemoryEvidence {
        MemoryEvidence {
            id: "evidence:1".to_string(),
            supports: vec!["claim:target".to_string()],
            text: "Evidence".to_string(),
            source: "source".to_string(),
            time: None,
            metadata: Default::default(),
        }
    }

    fn coordinate() -> TemporalCoordinate {
        TemporalCoordinate {
            dimension: "timeline".to_string(),
            scope_id: "scope".to_string(),
            occurred_at: None,
            observed_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
            sequence: Some(2),
            rank: None,
            metadata: Default::default(),
        }
    }
}
