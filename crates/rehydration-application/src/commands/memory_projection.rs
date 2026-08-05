use std::collections::BTreeMap;

use rehydration_domain::{
    MemoryRelationType, NodeDetailProjection, NodeProjection, NodeRelationProjection, PortError,
    ProjectionMutation, Provenance, RelationExplanation, RelationSemanticClass, SourceKind,
};
use serde_json::Value;

use rehydration_domain::ContextUpdatedEvent;

use crate::commands::{UpdateContextChange, UpdateContextCommand};

/// Derives the projection mutations for an already-accepted context event.
///
/// This is the replay contract: rebuilding a read model from the append-only
/// event log applies exactly these mutations per event, in log order. The
/// anchor's `memory_work_item_id` property is reconstructed from the event's
/// idempotency key — for KMP memory ingest they are identical; the original
/// `work_item_id` is not persisted on the event.
pub fn projection_mutations_for_context_event(
    event: &ContextUpdatedEvent,
) -> Result<Vec<ProjectionMutation>, PortError> {
    let command = UpdateContextCommand {
        root_node_id: event.root_node_id.clone(),
        role: event.role.clone(),
        work_item_id: event.idempotency_key.clone().unwrap_or_default(),
        logical_digest: event.logical_digest.clone(),
        changes: event
            .changes
            .iter()
            .map(|change| UpdateContextChange {
                operation: change.operation.clone(),
                entity_kind: change.entity_kind.clone(),
                entity_id: change.entity_id.clone(),
                payload_json: change.payload_json.clone(),
                reason: change.reason.clone().unwrap_or_default(),
                scopes: change.scopes.clone(),
            })
            .collect(),
        expected_revision: None,
        expected_content_hash: None,
        idempotency_key: event.idempotency_key.clone(),
        requested_by: event.requested_by.clone(),
    };
    memory_projection_mutations(&command, event.revision, &event.content_hash)
}

pub(crate) fn memory_projection_mutations(
    command: &UpdateContextCommand,
    revision: u64,
    content_hash: &str,
) -> Result<Vec<ProjectionMutation>, PortError> {
    if !command
        .changes
        .iter()
        .any(|change| change.entity_kind.starts_with("memory_"))
    {
        return Ok(Vec::new());
    }

    let provenance = command
        .requested_by
        .as_ref()
        .map(|agent| Provenance::new(SourceKind::Agent).with_source_agent(agent.clone()));
    let mut mutations = vec![ensure_anchor_mutation(command, provenance.clone())];

    for change in &command.changes {
        match change.entity_kind.as_str() {
            "memory_dimension" => {
                mutations.extend(memory_dimension_mutations(
                    command,
                    change,
                    provenance.clone(),
                )?);
            }
            "memory_entry" => {
                mutations.extend(memory_entry_mutations(
                    command,
                    change,
                    revision,
                    content_hash,
                    provenance.clone(),
                )?);
            }
            "memory_relation" => {
                mutations.push(memory_relation_mutation(change)?);
            }
            "memory_evidence" => {
                mutations.extend(memory_evidence_mutations(
                    command,
                    change,
                    revision,
                    content_hash,
                    provenance.clone(),
                )?);
            }
            _ => {}
        }
    }

    Ok(mutations)
}

fn ensure_anchor_mutation(
    command: &UpdateContextCommand,
    provenance: Option<Provenance>,
) -> ProjectionMutation {
    let mut properties = BTreeMap::new();
    properties.insert("memory_about".to_string(), command.root_node_id.clone());
    properties.insert("memory_role".to_string(), command.role.clone());
    properties.insert(
        "memory_work_item_id".to_string(),
        command.work_item_id.clone(),
    );

    ProjectionMutation::EnsureNode(NodeProjection {
        node_id: command.root_node_id.clone(),
        node_kind: "memory_anchor".to_string(),
        title: command.root_node_id.clone(),
        summary: format!("Memory anchor for `{}`.", command.root_node_id),
        status: "ACTIVE".to_string(),
        labels: labels(["memory", "anchor"]),
        properties,
        provenance,
    })
}

fn memory_dimension_mutations(
    command: &UpdateContextCommand,
    change: &UpdateContextChange,
    provenance: Option<Provenance>,
) -> Result<Vec<ProjectionMutation>, PortError> {
    let payload = payload(change)?;
    let dimension_id = payload_string(&payload, "id").unwrap_or_else(|| change.entity_id.clone());
    let kind = payload_string(&payload, "kind").unwrap_or_else(|| "dimension".to_string());
    let title = payload_string(&payload, "title").unwrap_or_else(|| dimension_id.clone());
    let summary = payload_string(&payload, "summary")
        .unwrap_or_else(|| format!("Memory dimension `{kind}` for `{}`.", command.root_node_id));
    let mut properties = properties_from_payload(command, change, &payload)?;
    properties.insert("dimension_kind".to_string(), kind.clone());

    Ok(vec![
        ProjectionMutation::UpsertNode(NodeProjection {
            node_id: dimension_id.clone(),
            node_kind: "memory_dimension".to_string(),
            title,
            summary,
            status: "ACTIVE".to_string(),
            labels: labels(["memory", "dimension", kind.as_str()]),
            properties,
            provenance,
        }),
        structural_relation(
            &command.root_node_id,
            &dimension_id,
            "has_dimension",
            Some("Memory anchor includes this dimension."),
            None,
        ),
    ])
}

fn memory_entry_mutations(
    command: &UpdateContextCommand,
    change: &UpdateContextChange,
    revision: u64,
    content_hash: &str,
    provenance: Option<Provenance>,
) -> Result<Vec<ProjectionMutation>, PortError> {
    let payload = payload(change)?;
    let entry_id = payload_string(&payload, "id").unwrap_or_else(|| change.entity_id.clone());
    let kind = payload_string(&payload, "kind").unwrap_or_else(|| "entry".to_string());
    let text = payload_string(&payload, "text")
        .or_else(|| payload_string(&payload, "summary"))
        .unwrap_or_else(|| entry_id.clone());
    let title = payload_string(&payload, "title").unwrap_or_else(|| truncate(&text, 80));
    let mut properties = properties_from_payload(command, change, &payload)?;
    properties.insert("entry_kind".to_string(), kind.clone());

    let mut mutations = vec![
        ProjectionMutation::UpsertNode(NodeProjection {
            node_id: entry_id.clone(),
            node_kind: kind.clone(),
            title,
            summary: text.clone(),
            status: "ACTIVE".to_string(),
            labels: labels(["memory", "entry", kind.as_str()]),
            properties,
            provenance,
        }),
        ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
            node_id: entry_id.clone(),
            detail: text.clone(),
            content_hash: detail_content_hash(content_hash, &entry_id),
            revision,
        }),
        structural_relation(
            &command.root_node_id,
            &entry_id,
            "records",
            Some("Memory anchor records this entry."),
            None,
        ),
    ];

    for coordinate in payload
        .get("coordinates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(scope_id) = payload_string(coordinate, "scope_id") else {
            continue;
        };
        mutations.push(contains_entry_relation(&scope_id, &entry_id, coordinate));
    }

    Ok(mutations)
}

fn memory_relation_mutation(change: &UpdateContextChange) -> Result<ProjectionMutation, PortError> {
    let payload = payload(change)?;
    let source = required_payload_string(&payload, "from", change)?;
    let target = required_payload_string(&payload, "to", change)?;
    let relation_type = MemoryRelationType::new(required_payload_string(&payload, "rel", change)?)
        .map_err(|error| {
            PortError::InvalidState(format!(
                "memory change `{}` relation type is invalid: {error}",
                change.entity_id
            ))
        })?;
    let semantic_class = payload_string(&payload, "class")
        .or_else(|| payload_string(&payload, "semantic_class"))
        .as_deref()
        .map(parse_semantic_class)
        .transpose()?
        .unwrap_or(RelationSemanticClass::Evidential);
    let coordinate = payload.get("coordinate").filter(|value| value.is_object());
    let sequence = coordinate
        .and_then(|coordinate| payload_u32(coordinate, "sequence"))
        .or_else(|| payload_u32(&payload, "sequence"));
    let explanation = RelationExplanation::new(semantic_class)
        .with_optional_rationale(payload_string(&payload, "why"))
        .with_optional_motivation(payload_string(&payload, "motivation"))
        .with_optional_method(payload_string(&payload, "method"))
        .with_optional_decision_id(payload_string(&payload, "decision_id"))
        .with_optional_caused_by_node_id(payload_string(&payload, "caused_by_node_id"))
        .with_optional_evidence(payload_string(&payload, "evidence"))
        .with_optional_confidence(payload_string(&payload, "confidence"))
        .with_optional_dimension(
            coordinate.and_then(|coordinate| payload_string(coordinate, "dimension")),
        )
        .with_optional_scope_id(
            coordinate.and_then(|coordinate| payload_string(coordinate, "scope_id")),
        )
        .with_optional_occurred_at(
            coordinate.and_then(|coordinate| payload_string(coordinate, "occurred_at")),
        )
        .with_optional_observed_at(
            coordinate.and_then(|coordinate| payload_string(coordinate, "observed_at")),
        )
        .with_optional_ingested_at(
            coordinate.and_then(|coordinate| payload_string(coordinate, "ingested_at")),
        )
        .with_optional_valid_from(
            coordinate.and_then(|coordinate| payload_string(coordinate, "valid_from")),
        )
        .with_optional_valid_until(
            coordinate.and_then(|coordinate| payload_string(coordinate, "valid_until")),
        )
        .with_optional_sequence(sequence)
        .with_optional_rank(coordinate.and_then(|coordinate| payload_u32(coordinate, "rank")));

    Ok(ProjectionMutation::UpsertNodeRelation(Box::new(
        NodeRelationProjection {
            source_node_id: source,
            target_node_id: target,
            relation_type: relation_type.as_str().to_string(),
            explanation,
        },
    )))
}

fn memory_evidence_mutations(
    command: &UpdateContextCommand,
    change: &UpdateContextChange,
    revision: u64,
    content_hash: &str,
    provenance: Option<Provenance>,
) -> Result<Vec<ProjectionMutation>, PortError> {
    let payload = payload(change)?;
    let evidence_id = payload_string(&payload, "id").unwrap_or_else(|| change.entity_id.clone());
    let text = payload_string(&payload, "text")
        .or_else(|| payload_string(&payload, "detail"))
        .unwrap_or_else(|| payload.to_string());
    let title = payload_string(&payload, "title").unwrap_or_else(|| truncate(&text, 80));
    let mut properties = properties_from_payload(command, change, &payload)?;
    if let Some(source) = payload_string(&payload, "source") {
        properties.insert("source".to_string(), source);
    }

    let mut mutations = vec![
        ProjectionMutation::UpsertNode(NodeProjection {
            node_id: evidence_id.clone(),
            node_kind: "memory_evidence".to_string(),
            title,
            summary: text.clone(),
            status: "ACTIVE".to_string(),
            labels: labels(["memory", "evidence", "proof"]),
            properties,
            provenance,
        }),
        ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
            node_id: evidence_id.clone(),
            detail: text.clone(),
            content_hash: detail_content_hash(content_hash, &evidence_id),
            revision,
        }),
        structural_relation(
            &command.root_node_id,
            &evidence_id,
            "has_evidence",
            Some("Memory anchor includes this evidence item."),
            None,
        ),
    ];

    for supported in payload
        .get("supports")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        mutations.push(ProjectionMutation::UpsertNodeRelation(Box::new(
            NodeRelationProjection {
                source_node_id: evidence_id.clone(),
                target_node_id: supported.to_string(),
                relation_type: "supports".to_string(),
                explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                    .with_rationale("Evidence supports this memory entry.")
                    .with_evidence(text.clone()),
            },
        )));
    }

    Ok(mutations)
}

fn structural_relation(
    source: &str,
    target: &str,
    relation_type: &str,
    rationale: Option<&str>,
    sequence: Option<u32>,
) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        relation_type: relation_type.to_string(),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural)
            .with_optional_rationale(rationale.map(str::to_string))
            .with_optional_sequence(sequence),
    }))
}

fn contains_entry_relation(
    scope_id: &str,
    entry_id: &str,
    coordinate: &Value,
) -> ProjectionMutation {
    ProjectionMutation::UpsertNodeRelation(Box::new(NodeRelationProjection {
        source_node_id: scope_id.to_string(),
        target_node_id: entry_id.to_string(),
        relation_type: "contains_entry".to_string(),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural)
            .with_rationale("Memory scope contains this entry.")
            .with_optional_dimension(payload_string(coordinate, "dimension"))
            .with_scope_id(scope_id)
            .with_optional_occurred_at(payload_string(coordinate, "occurred_at"))
            .with_optional_observed_at(payload_string(coordinate, "observed_at"))
            .with_optional_ingested_at(payload_string(coordinate, "ingested_at"))
            .with_optional_valid_from(payload_string(coordinate, "valid_from"))
            .with_optional_valid_until(payload_string(coordinate, "valid_until"))
            .with_optional_sequence(payload_u32(coordinate, "sequence"))
            .with_optional_rank(payload_u32(coordinate, "rank")),
    }))
}

fn payload(change: &UpdateContextChange) -> Result<Value, PortError> {
    serde_json::from_str(&change.payload_json).map_err(|error| {
        PortError::InvalidState(format!(
            "memory change `{}` payload is not valid JSON: {error}",
            change.entity_id
        ))
    })
}

fn properties_from_payload(
    command: &UpdateContextCommand,
    change: &UpdateContextChange,
    payload: &Value,
) -> Result<BTreeMap<String, String>, PortError> {
    let mut properties = BTreeMap::new();
    properties.insert("memory_about".to_string(), command.root_node_id.clone());
    properties.insert("memory_role".to_string(), command.role.clone());
    properties.insert(
        "memory_work_item_id".to_string(),
        command.work_item_id.clone(),
    );
    properties.insert("memory_entity_kind".to_string(), change.entity_kind.clone());
    properties.insert("memory_payload_json".to_string(), payload.to_string());

    if let Some(reason) = (!change.reason.is_empty()).then_some(change.reason.as_str()) {
        properties.insert("memory_change_reason".to_string(), reason.to_string());
    }

    if let Some(object) = payload.as_object() {
        for (key, value) in object {
            properties.insert(format!("payload_{key}"), property_value(value)?);
        }
    }

    Ok(properties)
}

fn property_value(value: &Value) -> Result<String, PortError> {
    Ok(match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).map_err(|error| {
            PortError::InvalidState(format!(
                "memory payload property could not serialize: {error}"
            ))
        })?,
    })
}

fn required_payload_string(
    payload: &Value,
    key: &str,
    change: &UpdateContextChange,
) -> Result<String, PortError> {
    payload_string(payload, key).ok_or_else(|| {
        PortError::InvalidState(format!(
            "memory change `{}` is missing required payload field `{key}`",
            change.entity_id
        ))
    })
}

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn payload_u32(payload: &Value, key: &str) -> Option<u32> {
    payload
        .get(key)
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
}

fn parse_semantic_class(value: &str) -> Result<RelationSemanticClass, PortError> {
    RelationSemanticClass::parse(value).map_err(|error| {
        PortError::InvalidState(format!(
            "memory relation semantic class is invalid: {error}"
        ))
    })
}

fn detail_content_hash(content_hash: &str, node_id: &str) -> String {
    format!("{content_hash}:{node_id}")
}

fn labels(values: impl IntoIterator<Item = impl AsRef<str>>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.as_ref().trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use rehydration_domain::{ContextEventChange, ContextUpdatedEvent, ProjectionMutation};

    use super::{memory_projection_mutations, projection_mutations_for_context_event};
    use crate::commands::{UpdateContextChange, UpdateContextCommand};

    fn entry_change_payload() -> String {
        serde_json::json!({
            "id": "claim:replayed",
            "kind": "claim",
            "text": "Replayed decision.",
            "coordinates": [{
                "dimension": "conversation",
                "scope_id": "about:question:r:dimension:conversation:s1",
                "occurred_at": "2026-07-01T10:00:00Z",
                "sequence": 1
            }]
        })
        .to_string()
    }

    fn sample_event() -> ContextUpdatedEvent {
        ContextUpdatedEvent {
            root_node_id: "question:r".to_string(),
            role: "memory".to_string(),
            revision: 3,
            content_hash: "hash-3".to_string(),
            changes: vec![ContextEventChange {
                operation: "UPSERT".to_string(),
                entity_kind: "memory_entry".to_string(),
                entity_id: "claim:replayed".to_string(),
                payload_json: entry_change_payload(),
                reason: Some("KMP memory entry ingest".to_string()),
                scopes: vec!["about:question:r:dimension:conversation:s1".to_string()],
            }],
            idempotency_key: Some("ingest:replay-test".to_string()),
            logical_digest: None,
            requested_by: Some("agent-r".to_string()),
            occurred_at: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn event_derivation_matches_command_derivation() {
        let event = sample_event();
        let command = UpdateContextCommand {
            root_node_id: event.root_node_id.clone(),
            role: event.role.clone(),
            work_item_id: "ingest:replay-test".to_string(),
            changes: vec![UpdateContextChange {
                operation: "UPSERT".to_string(),
                entity_kind: "memory_entry".to_string(),
                entity_id: "claim:replayed".to_string(),
                payload_json: entry_change_payload(),
                reason: "KMP memory entry ingest".to_string(),
                scopes: vec!["about:question:r:dimension:conversation:s1".to_string()],
            }],
            expected_revision: None,
            expected_content_hash: None,
            idempotency_key: Some("ingest:replay-test".to_string()),
            logical_digest: None,
            requested_by: Some("agent-r".to_string()),
        };

        let from_event =
            projection_mutations_for_context_event(&event).expect("event should derive");
        let from_command = memory_projection_mutations(&command, event.revision, "hash-3")
            .expect("command should derive");

        assert_eq!(from_event, from_command);
        assert!(!from_event.is_empty());
        assert!(from_event.iter().any(|mutation| matches!(
            mutation,
            ProjectionMutation::UpsertNodeDetail(detail) if detail.node_id == "claim:replayed"
                && detail.revision == 3
        )));
    }

    #[test]
    fn event_derivation_skips_non_memory_changes() {
        let mut event = sample_event();
        event.changes[0].entity_kind = "workspace_note".to_string();

        let mutations =
            projection_mutations_for_context_event(&event).expect("event should derive");

        assert!(mutations.is_empty());
    }
}
