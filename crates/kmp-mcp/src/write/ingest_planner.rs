use kmp_application::{
    validate_ref_token, validate_supplied_entry_ref, validate_supplied_evidence_ref,
};
use serde_json::Value;

use std::collections::BTreeMap;

use super::accepted_counts::AcceptedCounts;
use super::ingest_arguments::*;
use super::ingest_change::KmpIngestChange;
use super::ingest_plan::KmpIngestPlan;
use super::ingest_validation::*;

pub(crate) fn build_ingest_plan(arguments: &Value) -> Result<KmpIngestPlan, String> {
    let arguments = arguments
        .as_object()
        .ok_or_else(|| "tool arguments must be a JSON object".to_string())?;
    let about = required_string(arguments.get("about"), "about")?;
    validate_ref_token("about", &about)?;
    let idempotency_key = required_string(arguments.get("idempotency_key"), "idempotency_key")?;
    let memory = arguments
        .get("memory")
        .and_then(Value::as_object)
        .ok_or_else(|| "missing required object argument `memory`".to_string())?;

    let dimensions = required_array(memory.get("dimensions"), "memory.dimensions")?;
    let entries = required_array(memory.get("entries"), "memory.entries")?;
    let relations = optional_array(memory.get("relations"), "memory.relations")?;
    let evidence = optional_array(memory.get("evidence"), "memory.evidence")?;
    let provenance = arguments.get("provenance").and_then(Value::as_object);
    if let Some(provenance) = provenance {
        validate_provenance(provenance)?;
    }
    let mut dimension_kinds = BTreeMap::new();
    let mut changes = Vec::new();
    for dimension in dimensions {
        let id = required_object_string(dimension, "memory.dimensions[].id")?;
        validate_ref_token("memory.dimensions[].id", id)?;
        let kind = required_object_string(dimension, "memory.dimensions[].kind")?;
        if dimension_kinds.insert(id, kind).is_some() {
            return Err(format!("duplicate memory dimension `{id}`"));
        }
        changes.push(KmpIngestChange {
            entity_kind: "memory_dimension".to_string(),
            entity_id: id.to_string(),
            payload_json: stable_payload_json(dimension)?,
            reason: "KMP memory dimension ingest".to_string(),
            scopes: vec![id.to_string()],
        });
    }

    for entry in entries {
        let id = required_object_string(entry, "memory.entries[].id")?;
        validate_supplied_entry_ref(&about, "memory.entries[].id", id)?;
        let _kind = required_object_string(entry, "memory.entries[].kind")?;
        let _text = required_object_string(entry, "memory.entries[].text")?;
        validate_entry_positions(entry, &dimension_kinds)?;
        changes.push(KmpIngestChange {
            entity_kind: "memory_entry".to_string(),
            entity_id: id.to_string(),
            payload_json: stable_payload_json(entry)?,
            reason: "KMP memory entry ingest".to_string(),
            scopes: entry_scopes(entry),
        });
    }

    for relation in relations {
        let from = required_object_string(relation, "memory.relations[].from")?;
        let to = required_object_string(relation, "memory.relations[].to")?;
        validate_ingest_member_ref(&about, "memory.relations[].from", from, &dimension_kinds)?;
        validate_ingest_member_ref(&about, "memory.relations[].to", to, &dimension_kinds)?;
        for field in ["decision_id", "caused_by_node_id"] {
            if let Some(reference) = relation.get(field).and_then(Value::as_str) {
                validate_ingest_member_ref(
                    &about,
                    &format!("memory.relations[].{field}"),
                    reference,
                    &dimension_kinds,
                )?;
            }
        }
        let rel = required_object_string(relation, "memory.relations[].rel")?;
        let semantic_class = required_object_string(relation, "memory.relations[].class")?;
        validate_semantic_class(semantic_class)?;
        validate_relation_explanation(relation, semantic_class)?;
        if let Some(coordinate) = relation.get("coordinate") {
            validate_coordinate(
                coordinate,
                &dimension_kinds,
                "memory.relations[].coordinate",
            )?;
        }
        changes.push(KmpIngestChange {
            entity_kind: "memory_relation".to_string(),
            entity_id: format!("relation:{from}:{rel}:{to}"),
            payload_json: stable_payload_json(relation)?,
            reason: relation
                .get("why")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("KMP memory relation ingest")
                .to_string(),
            scopes: vec![from.to_string(), to.to_string()],
        });
    }

    for evidence_item in evidence {
        let id = required_object_string(evidence_item, "memory.evidence[].id")?;
        validate_supplied_evidence_ref(&about, "memory.evidence[].id", id)?;
        let _text = required_object_string(evidence_item, "memory.evidence[].text")?;
        validate_evidence_supports(&about, evidence_item, &dimension_kinds)?;
        changes.push(KmpIngestChange {
            entity_kind: "memory_evidence".to_string(),
            entity_id: id.to_string(),
            payload_json: stable_payload_json(evidence_item)?,
            reason: evidence_item
                .get("source")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("KMP memory evidence ingest")
                .to_string(),
            scopes: evidence_scopes(evidence_item),
        });
    }

    Ok(KmpIngestPlan {
        about,
        memory_id: memory_id_from_idempotency_key(&idempotency_key),
        idempotency_key,
        requested_by: provenance
            .and_then(|provenance| {
                provenance
                    .get("source_agent")
                    .or_else(|| provenance.get("source_kind"))
            })
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string),
        correlation_id: provenance
            .and_then(|provenance| provenance.get("correlation_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string),
        causation_id: provenance
            .and_then(|provenance| provenance.get("causation_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string),
        dry_run: arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        accepted: AcceptedCounts {
            entries: entries.len(),
            relations: relations.len(),
            evidence: evidence.len(),
        },
        changes,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn build_ingest_plan_translates_memory_to_command_changes() {
        let plan = build_ingest_plan(&sample_ingest_request()).expect("ingest plan should build");

        assert_eq!(plan.about, "question:830ce83f");
        assert_eq!(plan.memory_id, "memory:830ce83f:1");
        assert_eq!(plan.idempotency_key, "ingest:830ce83f:1");
        assert_eq!(plan.requested_by.as_deref(), Some("longmemeval-adapter"));
        assert_eq!(plan.correlation_id.as_deref(), Some("corr:830ce83f"));
        assert_eq!(plan.causation_id.as_deref(), Some("eval:item:830ce83f"));
        assert_eq!(
            plan.accepted,
            AcceptedCounts {
                entries: 1,
                relations: 1,
                evidence: 1
            }
        );
        assert_eq!(
            plan.changes
                .iter()
                .map(|change| change.entity_kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                "memory_dimension",
                "memory_entry",
                "memory_relation",
                "memory_evidence"
            ]
        );
        assert_eq!(plan.changes[1].scopes, vec!["conversation:rachel"]);
        assert_eq!(
            plan.changes[2].entity_id,
            "relation:question:830ce83f:claim:rachel-austin:supersedes:question:830ce83f:claim:rachel-denver"
        );
    }

    #[test]
    fn build_ingest_plan_rejects_missing_memory_shape() {
        let error = build_ingest_plan(&json!({
            "about": "question:830ce83f",
            "idempotency_key": "ingest:830ce83f:1"
        }))
        .expect_err("missing memory should fail");

        assert_eq!(error, "missing required object argument `memory`");
    }

    #[test]
    fn build_ingest_plan_rejects_unknown_entry_position_scope() {
        let error = build_ingest_plan(&json!({
            "about": "question:830ce83f",
            "memory": {
                "dimensions": [
                    {
                        "id": "conversation:rachel",
                        "kind": "conversation"
                    }
                ],
                "entries": [
                    {
                        "id": "question:830ce83f:claim:rachel-austin",
                        "kind": "claim",
                        "text": "Rachel moved to Austin.",
                        "coordinates": [
                            {
                                "dimension": "conversation",
                                "scope_id": "conversation:missing",
                                "sequence": 1
                            }
                        ]
                    }
                ]
            },
            "idempotency_key": "ingest:830ce83f:1"
        }))
        .expect_err("unknown position scope should fail");

        assert_eq!(
            error,
            "memory.entries[].coordinates[] references unknown dimension scope `conversation:missing`"
        );
    }

    #[test]
    fn build_ingest_plan_rejects_coordinate_kind_mismatch() {
        let mut request = sample_ingest_request();
        request["memory"]["entries"][0]["coordinates"][0]["dimension"] = json!("ceremony");

        let error = build_ingest_plan(&request).expect_err("coordinate kind mismatch should fail");

        assert_eq!(
            error,
            "memory.entries[].coordinates[].dimension `ceremony` does not match declared kind `conversation` for scope `conversation:rachel`"
        );
    }

    #[test]
    fn build_ingest_plan_rejects_relation_coordinate_kind_mismatch() {
        let mut request = sample_ingest_request();
        request["memory"]["relations"][0]["coordinate"] = json!({
            "dimension": "ceremony",
            "scope_id": "conversation:rachel",
            "valid_from": "2026-04-12T15:00:00Z"
        });

        let error =
            build_ingest_plan(&request).expect_err("relation coordinate kind mismatch should fail");

        assert_eq!(
            error,
            "memory.relations[].coordinate.dimension `ceremony` does not match declared kind `conversation` for scope `conversation:rachel`"
        );
    }

    #[test]
    fn build_ingest_plan_bounds_every_caller_supplied_ref_field() {
        const HOSTILE_REFS: &[&str] = &[
            "incident:gamma:entry:observation:foreign",
            "incident:beta",
            "incident:alfa:entry:x\nincident:beta:entry:y",
            "../../incident:beta:entry:x",
        ];
        const REF_FIELDS: &[&str] = &[
            "entry.id",
            "relation.from",
            "relation.to",
            "relation.decision_id",
            "relation.caused_by_node_id",
            "evidence.id",
            "evidence.supports",
        ];

        for field in REF_FIELDS {
            for hostile in HOSTILE_REFS {
                let mut request = sample_ingest_request();
                request["about"] = json!("incident:alfa");
                request["memory"]["entries"][0]["id"] =
                    json!("incident:alfa:entry:observation:local");
                request["memory"]["relations"][0]["from"] =
                    json!("incident:alfa:entry:observation:local");
                request["memory"]["relations"][0]["to"] = json!("incident:alfa");
                request["memory"]["evidence"][0]["id"] =
                    json!("evidence:incident:alfa:entry:observation:local:current");
                request["memory"]["evidence"][0]["supports"][0] =
                    json!("incident:alfa:entry:observation:local");

                match *field {
                    "entry.id" => request["memory"]["entries"][0]["id"] = json!(hostile),
                    "relation.from" => request["memory"]["relations"][0]["from"] = json!(hostile),
                    "relation.to" => request["memory"]["relations"][0]["to"] = json!(hostile),
                    "relation.decision_id" => {
                        request["memory"]["relations"][0]["decision_id"] = json!(hostile)
                    }
                    "relation.caused_by_node_id" => {
                        request["memory"]["relations"][0]["caused_by_node_id"] = json!(hostile)
                    }
                    "evidence.id" => request["memory"]["evidence"][0]["id"] = json!(hostile),
                    "evidence.supports" => {
                        request["memory"]["evidence"][0]["supports"][0] = json!(hostile)
                    }
                    unexpected => panic!("unknown test field {unexpected}"),
                }

                let error = build_ingest_plan(&request)
                    .expect_err("an ingest ref outside the about must be refused");
                assert!(
                    error.contains("does not belong to about")
                        || error.contains("memory refs cannot contain"),
                    "{field} accepted or misclassified {hostile:?}: {error}"
                );
            }
        }
    }

    fn sample_ingest_request() -> Value {
        json!({
            "about": "question:830ce83f",
            "memory": {
                "dimensions": [
                    {
                        "id": "conversation:rachel",
                        "kind": "conversation"
                    }
                ],
                "entries": [
                    {
                        "id": "question:830ce83f:claim:rachel-austin",
                        "kind": "claim",
                        "text": "Rachel moved to Austin.",
                        "coordinates": [
                            {
                                "dimension": "conversation",
                                "scope_id": "conversation:rachel",
                                "sequence": 1
                            }
                        ]
                    }
                ],
                "relations": [
                    {
                        "from": "question:830ce83f:claim:rachel-austin",
                        "to": "question:830ce83f:claim:rachel-denver",
                        "rel": "supersedes",
                        "class": "evidential",
                        "why": "Later statement corrects earlier statement.",
                        "evidence": "Rachel corrected the destination.",
                        "confidence": "high"
                    }
                ],
                "evidence": [
                    {
                        "id": "evidence:question:830ce83f:claim:rachel-austin:current",
                        "supports": ["question:830ce83f:claim:rachel-austin"],
                        "text": "Rachel corrected the destination.",
                        "source": "conversation"
                    }
                ]
            },
            "provenance": {
                "source_kind": "agent",
                "source_agent": "longmemeval-adapter",
                "observed_at": "2026-05-04T10:00:00Z",
                "correlation_id": "corr:830ce83f",
                "causation_id": "eval:item:830ce83f"
            },
            "idempotency_key": "ingest:830ce83f:1"
        })
    }
}
