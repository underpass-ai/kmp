use kmp_proto::v1beta1::{
    IngestRequest, Memory, MemoryConfidence, MemoryDimension, MemoryEntry, MemoryEvidence,
    MemoryProvenance, MemoryRelation, MemoryRelationExplanation, MemorySemanticClass,
    TemporalCoordinate,
};
use serde_json::{Map, Value};

use super::common::{
    confidence_from_field, object, optional_array_field, optional_bool_field,
    optional_metadata_field, optional_object_field, optional_positive_u32_field,
    optional_string_array_field, optional_string_field, optional_timestamp_field,
    required_array_field, required_object_field, required_string_field, required_timestamp_field,
    semantic_class_from_field, source_kind_from_field,
};

pub(crate) fn ingest_request_from_arguments(arguments: &Value) -> Result<IngestRequest, String> {
    let arguments = object(arguments, "tool arguments")?;
    let about = required_string_field(arguments, "about", "about")?;
    let memory = memory_from_object(required_object_field(arguments, "memory", "memory")?)?;
    let provenance = optional_object_field(arguments, "provenance", "provenance")?
        .map(provenance_from_object)
        .transpose()?;
    let idempotency_key = required_string_field(arguments, "idempotency_key", "idempotency_key")?;
    let dry_run = optional_bool_field(arguments, "dry_run", "dry_run")?.unwrap_or(false);
    let label_policy = match arguments.get("label_policy").and_then(Value::as_str) {
        None | Some("warn") => kmp_proto::v1beta1::LabelPolicy::Warn,
        Some("refuse") => kmp_proto::v1beta1::LabelPolicy::Refuse,
        Some(other) => {
            return Err(format!(
                "label_policy must be `warn` or `refuse`, not `{other}`"
            ));
        }
    };

    Ok(IngestRequest {
        about,
        memory: Some(memory),
        provenance,
        idempotency_key,
        dry_run,
        label_policy: label_policy as i32,
    })
}

fn memory_from_object(memory: &Map<String, Value>) -> Result<Memory, String> {
    let dimensions = required_array_field_allow_empty(memory, "dimensions", "memory.dimensions")?;
    let entries = required_array_field(memory, "entries", "memory.entries")?;
    let relations = optional_array_field(memory, "relations", "memory.relations")?;
    let evidence = optional_array_field(memory, "evidence", "memory.evidence")?;

    Ok(Memory {
        dimensions: dimensions
            .iter()
            .map(dimension_from_value)
            .collect::<Result<Vec<_>, _>>()?,
        entries: entries
            .iter()
            .map(entry_from_value)
            .collect::<Result<Vec<_>, _>>()?,
        relations: relations
            .iter()
            .map(relation_from_value)
            .collect::<Result<Vec<_>, _>>()?,
        evidence: evidence
            .iter()
            .map(evidence_from_value)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn required_array_field_allow_empty<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a [Value], String> {
    object
        .get(key)
        .ok_or_else(|| format!("missing required array argument `{path}`"))
        .and_then(|value| {
            value
                .as_array()
                .map(Vec::as_slice)
                .ok_or_else(|| format!("argument `{path}` must be an array"))
        })
}

fn dimension_from_value(value: &Value) -> Result<MemoryDimension, String> {
    let value = object(value, "memory.dimensions[]")?;
    Ok(MemoryDimension {
        id: required_string_field(value, "id", "memory.dimensions[].id")?,
        kind: required_string_field(value, "kind", "memory.dimensions[].kind")?,
        title: optional_string_field(value, "title", "memory.dimensions[].title")?
            .unwrap_or_default(),
        metadata: optional_metadata_field(value, "metadata", "memory.dimensions[].metadata")?,
    })
}

fn entry_from_value(value: &Value) -> Result<MemoryEntry, String> {
    let value = object(value, "memory.entries[]")?;
    let coordinates = required_array_field(value, "coordinates", "memory.entries[].coordinates")?;

    Ok(MemoryEntry {
        id: required_string_field(value, "id", "memory.entries[].id")?,
        kind: required_string_field(value, "kind", "memory.entries[].kind")?,
        text: required_string_field(value, "text", "memory.entries[].text")?,
        coordinates: coordinates
            .iter()
            .map(|coordinate| coordinate_from_value(coordinate, "memory.entries[].coordinates[]"))
            .collect::<Result<Vec<_>, _>>()?,
        metadata: optional_metadata_field(value, "metadata", "memory.entries[].metadata")?,
    })
}

fn coordinate_from_value(value: &Value, path: &str) -> Result<TemporalCoordinate, String> {
    let value = object(value, path)?;
    Ok(TemporalCoordinate {
        dimension: required_string_field(value, "dimension", &format!("{path}.dimension"))?,
        scope_id: required_string_field(value, "scope_id", &format!("{path}.scope_id"))?,
        occurred_at: optional_timestamp_field(
            value,
            "occurred_at",
            &format!("{path}.occurred_at"),
        )?,
        observed_at: optional_timestamp_field(
            value,
            "observed_at",
            &format!("{path}.observed_at"),
        )?,
        ingested_at: optional_timestamp_field(
            value,
            "ingested_at",
            &format!("{path}.ingested_at"),
        )?,
        valid_from: optional_timestamp_field(value, "valid_from", &format!("{path}.valid_from"))?,
        valid_until: optional_timestamp_field(
            value,
            "valid_until",
            &format!("{path}.valid_until"),
        )?,
        sequence: optional_positive_u32_field(value, "sequence", &format!("{path}.sequence"))?,
        rank: optional_positive_u32_field(value, "rank", &format!("{path}.rank"))?,
        metadata: optional_metadata_field(value, "metadata", &format!("{path}.metadata"))?,
    })
}

fn relation_from_value(value: &Value) -> Result<MemoryRelation, String> {
    let value = object(value, "memory.relations[]")?;
    let semantic_class = semantic_class_from_field(value, "class", "memory.relations[].class")?;
    let why = optional_string_field(value, "why", "memory.relations[].why")?.unwrap_or_default();
    let evidence = optional_string_field(value, "evidence", "memory.relations[].evidence")?
        .unwrap_or_default();
    let confidence = confidence_from_field(value, "confidence", "memory.relations[].confidence")?;
    let coordinate = optional_object_field(value, "coordinate", "memory.relations[].coordinate")?
        .map(|coordinate| {
            coordinate_from_value(
                &Value::Object(coordinate.clone()),
                "memory.relations[].coordinate",
            )
        })
        .transpose()?;

    if semantic_class != MemorySemanticClass::Structural as i32 {
        if confidence == MemoryConfidence::Unspecified as i32 {
            return Err("non-structural memory relations require confidence".to_string());
        }
        if why.trim().is_empty() && evidence.trim().is_empty() {
            return Err("non-structural memory relations require why or evidence".to_string());
        }
    }

    Ok(MemoryRelation {
        source_ref: required_string_field(value, "from", "memory.relations[].from")?,
        target_ref: required_string_field(value, "to", "memory.relations[].to")?,
        rel: required_string_field(value, "rel", "memory.relations[].rel")?,
        semantic_class,
        why,
        evidence,
        confidence,
        sequence: optional_positive_u32_field(value, "sequence", "memory.relations[].sequence")?,
        evidence_refs: Vec::new(),
        explanation: Some(MemoryRelationExplanation {
            motivation: optional_string_field(
                value,
                "motivation",
                "memory.relations[].motivation",
            )?
            .unwrap_or_default(),
            method: optional_string_field(value, "method", "memory.relations[].method")?
                .unwrap_or_default(),
            decision_id: optional_string_field(
                value,
                "decision_id",
                "memory.relations[].decision_id",
            )?
            .unwrap_or_default(),
            caused_by_node_id: optional_string_field(
                value,
                "caused_by_node_id",
                "memory.relations[].caused_by_node_id",
            )?
            .unwrap_or_default(),
            coordinate,
        }),
    })
}

fn evidence_from_value(value: &Value) -> Result<MemoryEvidence, String> {
    let value = object(value, "memory.evidence[]")?;
    Ok(MemoryEvidence {
        id: required_string_field(value, "id", "memory.evidence[].id")?,
        supports: optional_string_array_field(value, "supports", "memory.evidence[].supports")?,
        text: required_string_field(value, "text", "memory.evidence[].text")?,
        source: optional_string_field(value, "source", "memory.evidence[].source")?
            .unwrap_or_default(),
        time: optional_timestamp_field(value, "time", "memory.evidence[].time")?,
        metadata: optional_metadata_field(value, "metadata", "memory.evidence[].metadata")?,
    })
}

pub(super) fn provenance_from_object(
    value: &Map<String, Value>,
) -> Result<MemoryProvenance, String> {
    Ok(MemoryProvenance {
        source_kind: source_kind_from_field(value, "source_kind", "provenance.source_kind")?,
        source_agent: required_string_field(value, "source_agent", "provenance.source_agent")?,
        observed_at: Some(required_timestamp_field(
            value,
            "observed_at",
            "provenance.observed_at",
        )?),
        correlation_id: optional_string_field(
            value,
            "correlation_id",
            "provenance.correlation_id",
        )?
        .unwrap_or_default(),
        causation_id: optional_string_field(value, "causation_id", "provenance.causation_id")?
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use kmp_proto::v1beta1::MemorySourceKind;
    use serde_json::json;

    use super::*;

    #[test]
    fn ingest_request_maps_mcp_memory_to_kernel_memory_service_proto() {
        let request = ingest_request_from_arguments(&json!({
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
                        "id": "claim:rachel-austin",
                        "kind": "claim",
                        "text": "Rachel moved to Austin.",
                        "coordinates": [
                            {
                                "dimension": "conversation",
                                "scope_id": "conversation:rachel",
                                "sequence": 1,
                                "occurred_at": "2026-04-12T15:05:00Z"
                            }
                        ]
                    }
                ],
                "relations": [
                    {
                        "from": "claim:rachel-austin",
                        "to": "claim:rachel-denver",
                        "rel": "supersedes",
                        "class": "evidential",
                        "why": "Later statement corrects earlier statement.",
                        "confidence": "high"
                    }
                ],
                "evidence": [
                    {
                        "id": "evidence:rachel",
                        "supports": ["claim:rachel-austin"],
                        "text": "Rachel corrected the destination."
                    }
                ]
            },
            "provenance": {
                "source_kind": "agent",
                "source_agent": "longmemeval-adapter",
                "observed_at": "2026-05-04T10:00:00Z"
            },
            "idempotency_key": "ingest:830ce83f:1"
        }))
        .expect("ingest request should map");

        assert_eq!(request.about, "question:830ce83f");
        assert_eq!(
            request.memory.as_ref().expect("memory").relations[0].source_ref,
            "claim:rachel-austin"
        );
        assert_eq!(
            request.provenance.expect("provenance").source_kind,
            MemorySourceKind::Agent as i32
        );
    }

    #[test]
    fn ingest_request_allows_empty_dimensions_for_incremental_append() {
        let request = ingest_request_from_arguments(&json!({
            "about": "question:830ce83f",
            "memory": {
                "dimensions": [],
                "entries": [
                    {
                        "id": "claim:rachel-denver",
                        "kind": "claim",
                        "text": "Rachel moved to Denver.",
                        "coordinates": [
                            {
                                "dimension": "conversation",
                                "scope_id": "conversation:rachel",
                                "sequence": 2,
                                "occurred_at": "2026-04-13T15:05:00Z"
                            }
                        ]
                    }
                ],
                "relations": [],
                "evidence": []
            },
            "idempotency_key": "ingest:830ce83f:2"
        }))
        .expect("incremental append should map");

        let memory = request.memory.expect("memory");
        assert!(memory.dimensions.is_empty());
        assert_eq!(memory.entries[0].id, "claim:rachel-denver");
    }
}
