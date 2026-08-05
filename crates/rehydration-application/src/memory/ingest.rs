use std::collections::{BTreeMap, BTreeSet};

use rehydration_domain::{
    MemoryDimensionIdentity, MemoryRelationType, RelationSemanticClass, SourceKind,
};

use crate::ApplicationError;
use crate::commands::{UpdateContextChange, UpdateContextCommand};
use crate::memory::{
    MemoryAcceptedCounts, MemoryCoordinateData, MemoryData, MemoryDimensionData,
    MemoryIngestCommand, MemoryIngestOutcome,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExistingMemoryRefs {
    pub refs: BTreeSet<String>,
    pub dimensions: BTreeSet<String>,
}

pub fn translate_memory_ingest(
    command: &MemoryIngestCommand,
    existing: &ExistingMemoryRefs,
) -> Result<(UpdateContextCommand, MemoryIngestOutcome), ApplicationError> {
    validate_command(command)?;
    let memory = namespaced_memory(&command.about, &command.memory, existing)?;

    let changes = memory_changes(&memory)?;
    let outcome = MemoryIngestOutcome {
        about: command.about.clone(),
        memory_id: memory_id_from_idempotency_key(&command.idempotency_key),
        accepted: MemoryAcceptedCounts {
            entries: command.memory.entries.len(),
            relations: command.memory.relations.len(),
            evidence: command.memory.evidence.len(),
        },
        read_after_write_ready: false,
        warnings: Vec::new(),
    };

    Ok((
        UpdateContextCommand {
            root_node_id: command.about.clone(),
            role: "memory".to_string(),
            work_item_id: command.idempotency_key.clone(),
            changes,
            expected_revision: None,
            expected_content_hash: None,
            idempotency_key: Some(command.idempotency_key.clone()),
            logical_digest: Some(logical_digest(command)),
            requested_by: command
                .provenance
                .as_ref()
                .map(|provenance| provenance.source_agent.clone()),
        },
        outcome,
    ))
}

fn validate_command(command: &MemoryIngestCommand) -> Result<(), ApplicationError> {
    require_non_empty(&command.about, "about")?;
    require_non_empty(&command.idempotency_key, "idempotency_key")?;
    if let Some(provenance) = command.provenance.as_ref() {
        SourceKind::parse(&provenance.source_kind).map_err(|error| {
            ApplicationError::Validation(format!(
                "memory provenance source_kind is invalid: {error}"
            ))
        })?;
        require_non_empty(&provenance.source_agent, "provenance.source_agent")?;
        require_non_empty(&provenance.observed_at, "provenance.observed_at")?;
    }

    Ok(())
}

fn namespaced_memory(
    about: &str,
    memory: &MemoryData,
    existing: &ExistingMemoryRefs,
) -> Result<MemoryData, ApplicationError> {
    if memory.dimensions.is_empty() && existing.dimensions.is_empty() {
        return Err(ApplicationError::Validation(
            "memory.dimensions must not be empty when no existing memory dimensions are available"
                .to_string(),
        ));
    }
    if memory.entries.is_empty() {
        return Err(ApplicationError::Validation(
            "memory.entries must not be empty".to_string(),
        ));
    }

    let mut known_refs = existing.refs.clone();
    known_refs.extend(existing.dimensions.iter().cloned());
    let mut dimension_ids = existing.dimensions.clone();
    let mut dimension_aliases = existing_dimension_aliases(about, existing);
    let mut declared_dimension_kinds = BTreeMap::new();
    let mut declared_dimension_refs = BTreeSet::new();
    let mut dimensions = Vec::new();
    for dimension in &memory.dimensions {
        require_non_empty(&dimension.id, "memory.dimensions[].id")?;
        require_non_empty(&dimension.kind, "memory.dimensions[].kind")?;
        let dimension_identity = dimension_identity(about, &dimension.id)?;
        let dimension_ref = dimension_identity.node_id();
        declared_dimension_kinds.insert(dimension_ref.clone(), dimension.kind.clone());
        insert_unique(
            &mut declared_dimension_refs,
            &dimension_ref,
            "memory dimension",
        )?;
        if existing.dimensions.contains(&dimension_ref) {
            dimension_aliases
                .entry(dimension.id.clone())
                .or_insert_with(|| dimension_ref.clone());
            known_refs.insert(dimension_ref);
            continue;
        }
        insert_unique(&mut dimension_ids, &dimension_ref, "memory dimension")?;
        if dimension_aliases
            .insert(dimension.id.clone(), dimension_ref.clone())
            .is_some()
        {
            return Err(ApplicationError::Validation(format!(
                "duplicate memory dimension `{}`",
                dimension.id
            )));
        }
        known_refs.insert(dimension_ref.clone());

        let mut metadata = dimension.metadata.clone();
        metadata
            .entry("memory_about".to_string())
            .or_insert_with(|| about.to_string());
        metadata
            .entry("memory_dimension_id".to_string())
            .or_insert_with(|| dimension.id.clone());
        dimensions.push(MemoryDimensionData {
            id: dimension_ref,
            kind: dimension.kind.clone(),
            title: dimension.title.clone(),
            metadata,
        });
    }

    let mut entry_ids = BTreeSet::new();
    let mut entries = Vec::new();
    for entry in &memory.entries {
        require_non_empty(&entry.id, "memory.entries[].id")?;
        require_non_empty(&entry.kind, "memory.entries[].kind")?;
        require_non_empty(&entry.text, "memory.entries[].text")?;
        if entry.coordinates.is_empty() {
            return Err(ApplicationError::Validation(format!(
                "memory entry `{}` must include at least one coordinate",
                entry.id
            )));
        }
        insert_unique(&mut entry_ids, &entry.id, "memory entry")?;
        known_refs.insert(entry.id.clone());

        let mut coordinates = Vec::new();
        for coordinate in &entry.coordinates {
            coordinates.push(normalize_coordinate(
                coordinate,
                "memory.entries[].coordinates[]",
                "memory entry",
                &dimension_aliases,
                &dimension_ids,
                &declared_dimension_kinds,
            )?);
        }
        let mut entry = entry.clone();
        entry.coordinates = coordinates;
        entries.push(entry);
    }

    let mut relations = Vec::new();
    for relation in &memory.relations {
        require_non_empty(&relation.source_ref, "memory.relations[].source_ref")?;
        require_non_empty(&relation.target_ref, "memory.relations[].target_ref")?;
        require_non_empty(&relation.rel, "memory.relations[].rel")?;
        let relation_type = MemoryRelationType::new(&relation.rel).map_err(|error| {
            ApplicationError::Validation(format!("memory relation type is invalid: {error}"))
        })?;
        let semantic_class =
            RelationSemanticClass::parse(&relation.semantic_class).map_err(|error| {
                ApplicationError::Validation(format!("memory relation class is invalid: {error}"))
            })?;
        let source_ref = normalize_ref(&relation.source_ref, &dimension_aliases);
        let target_ref = normalize_ref(&relation.target_ref, &dimension_aliases);
        if !known_refs.contains(&source_ref) || !known_refs.contains(&target_ref) {
            return Err(ApplicationError::Validation(format!(
                "memory relation `{}` -> `{}` references unknown refs",
                relation.source_ref, relation.target_ref
            )));
        }
        if semantic_class != RelationSemanticClass::Structural {
            if relation
                .confidence
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err(ApplicationError::Validation(
                    "non-structural memory relations require confidence".to_string(),
                ));
            }
            if relation.why.as_deref().unwrap_or("").trim().is_empty()
                && relation.evidence.as_deref().unwrap_or("").trim().is_empty()
            {
                return Err(ApplicationError::Validation(
                    "non-structural memory relations require why or evidence".to_string(),
                ));
            }
        }
        validate_positive_optional(relation.sequence, "memory.relations[].sequence")?;
        let coordinate = relation
            .coordinate
            .as_ref()
            .map(|coordinate| {
                normalize_coordinate(
                    coordinate,
                    "memory.relations[].coordinate",
                    "memory relation",
                    &dimension_aliases,
                    &dimension_ids,
                    &declared_dimension_kinds,
                )
            })
            .transpose()?;
        let mut relation = relation.clone();
        relation.source_ref = source_ref;
        relation.target_ref = target_ref;
        relation.rel = relation_type.as_str().to_string();
        relation.coordinate = coordinate;
        relations.push(relation);
    }

    let mut evidence_ids = BTreeSet::new();
    let mut evidence_items = Vec::new();
    for evidence in &memory.evidence {
        require_non_empty(&evidence.id, "memory.evidence[].id")?;
        require_non_empty(&evidence.text, "memory.evidence[].text")?;
        insert_unique(&mut evidence_ids, &evidence.id, "memory evidence")?;
        known_refs.insert(evidence.id.clone());
        let mut supports = Vec::new();
        for supported in &evidence.supports {
            require_non_empty(supported, "memory.evidence[].supports[]")?;
            let supported_ref = normalize_ref(supported, &dimension_aliases);
            if !known_refs.contains(&supported_ref) {
                return Err(ApplicationError::Validation(format!(
                    "memory evidence `{}` supports unknown ref `{supported}`",
                    evidence.id
                )));
            }
            supports.push(supported_ref);
        }
        let mut evidence = evidence.clone();
        evidence.supports = supports;
        evidence_items.push(evidence);
    }

    Ok(MemoryData {
        dimensions,
        entries,
        relations,
        evidence: evidence_items,
    })
}

fn existing_dimension_aliases(
    about: &str,
    existing: &ExistingMemoryRefs,
) -> BTreeMap<String, String> {
    existing
        .dimensions
        .iter()
        .filter_map(|dimension_ref| {
            let identity = MemoryDimensionIdentity::parse(dimension_ref)?;
            (identity.about() == about)
                .then(|| (identity.dimension_id().to_string(), dimension_ref.clone()))
        })
        .collect()
}

fn dimension_identity(
    about: &str,
    dimension_id: &str,
) -> Result<MemoryDimensionIdentity, ApplicationError> {
    MemoryDimensionIdentity::new(about, dimension_id)
        .map_err(|error| ApplicationError::Validation(error.to_string()))
}

fn normalize_ref(value: &str, dimension_aliases: &BTreeMap<String, String>) -> String {
    dimension_aliases
        .get(value)
        .cloned()
        .unwrap_or_else(|| value.to_string())
}

fn normalize_coordinate(
    coordinate: &MemoryCoordinateData,
    field: &str,
    label: &str,
    dimension_aliases: &BTreeMap<String, String>,
    dimension_ids: &BTreeSet<String>,
    declared_dimension_kinds: &BTreeMap<String, String>,
) -> Result<MemoryCoordinateData, ApplicationError> {
    require_non_empty(&coordinate.dimension, &format!("{field}.dimension"))?;
    require_non_empty(&coordinate.scope_id, &format!("{field}.scope_id"))?;
    let scope_id = normalize_ref(&coordinate.scope_id, dimension_aliases);
    if !dimension_ids.contains(&scope_id) {
        return Err(ApplicationError::Validation(format!(
            "{label} coordinate references unknown dimension scope `{}`",
            coordinate.scope_id
        )));
    }
    if let Some(expected_kind) = declared_dimension_kinds.get(&scope_id)
        && coordinate.dimension != *expected_kind
    {
        return Err(ApplicationError::Validation(format!(
            "{label} coordinate dimension `{}` does not match declared kind `{expected_kind}` for scope `{}`",
            coordinate.dimension, coordinate.scope_id
        )));
    }
    validate_positive_optional(coordinate.sequence, &format!("{field}.sequence"))?;
    validate_positive_optional(coordinate.rank, &format!("{field}.rank"))?;

    let mut coordinate = coordinate.clone();
    coordinate.scope_id = scope_id;
    Ok(coordinate)
}

fn memory_changes(memory: &MemoryData) -> Result<Vec<UpdateContextChange>, ApplicationError> {
    let mut changes = Vec::new();
    for dimension in &memory.dimensions {
        changes.push(change(
            "memory_dimension",
            &dimension.id,
            serde_json::to_string(dimension),
            "KMP memory dimension ingest",
            vec![dimension.id.clone()],
        )?);
    }
    for entry in &memory.entries {
        let scopes = entry
            .coordinates
            .iter()
            .map(|coordinate| coordinate.scope_id.clone())
            .collect();
        changes.push(change(
            "memory_entry",
            &entry.id,
            serde_json::to_string(entry),
            "KMP memory entry ingest",
            scopes,
        )?);
    }
    for relation in &memory.relations {
        changes.push(change(
            "memory_relation",
            &format!(
                "relation:{}:{}:{}",
                relation.source_ref, relation.rel, relation.target_ref
            ),
            serde_json::to_string(relation),
            relation
                .why
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("KMP memory relation ingest"),
            vec![relation.source_ref.clone(), relation.target_ref.clone()],
        )?);
    }
    for evidence in &memory.evidence {
        changes.push(change(
            "memory_evidence",
            &evidence.id,
            serde_json::to_string(evidence),
            evidence
                .source
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("KMP memory evidence ingest"),
            evidence.supports.clone(),
        )?);
    }

    Ok(changes)
}

fn change(
    entity_kind: &str,
    entity_id: &str,
    payload: Result<String, serde_json::Error>,
    reason: &str,
    scopes: Vec<String>,
) -> Result<UpdateContextChange, ApplicationError> {
    Ok(UpdateContextChange {
        operation: "UPSERT".to_string(),
        entity_kind: entity_kind.to_string(),
        entity_id: entity_id.to_string(),
        payload_json: payload.map_err(|error| {
            ApplicationError::Validation(format!("memory payload could not serialize: {error}"))
        })?,
        reason: reason.to_string(),
        scopes,
    })
}

fn require_non_empty(value: &str, field: &str) -> Result<(), ApplicationError> {
    if value.trim().is_empty() {
        Err(ApplicationError::Validation(format!(
            "{field} cannot be empty"
        )))
    } else {
        Ok(())
    }
}

fn insert_unique(
    values: &mut BTreeSet<String>,
    value: &str,
    label: &str,
) -> Result<(), ApplicationError> {
    if !values.insert(value.to_string()) {
        Err(ApplicationError::Validation(format!(
            "duplicate {label} `{value}`"
        )))
    } else {
        Ok(())
    }
}

fn validate_positive_optional(value: Option<u32>, field: &str) -> Result<(), ApplicationError> {
    if value == Some(0) {
        Err(ApplicationError::Validation(format!(
            "{field} must be greater than zero when set"
        )))
    } else {
        Ok(())
    }
}

/// Digest of the logical ingest, taken before translation.
///
/// Translation consults existing state (a dimension already declared is not
/// re-created), so the same command translates differently after its own
/// first apply. This digest is computed from what the caller *said*, which is
/// the thing that must be equal for a replay to deserve a replayed answer.
fn logical_digest(command: &MemoryIngestCommand) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(command.about.as_bytes());
    hasher.update([0]);
    let memory = serde_json::to_vec(&command.memory)
        .expect("memory data serializes: it holds only strings, maps and integers");
    hasher.update(&memory);
    hasher.update([0]);
    if let Some(provenance) = &command.provenance {
        let provenance =
            serde_json::to_vec(provenance).expect("provenance serializes: it holds only strings");
        hasher.update(&provenance);
    }
    format!("{:x}", hasher.finalize())
}

fn memory_id_from_idempotency_key(idempotency_key: &str) -> String {
    idempotency_key
        .strip_prefix("ingest:")
        .map(|suffix| format!("memory:{suffix}"))
        .unwrap_or_else(|| format!("memory:{idempotency_key}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::ApplicationError;
    use crate::memory::{
        ExistingMemoryRefs, MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData,
        MemoryEvidenceData, MemoryIngestCommand, MemoryRelationData,
    };

    use super::translate_memory_ingest;

    #[test]
    fn translate_memory_ingest_creates_internal_memory_update_command() {
        let command = sample_command();

        let (update, outcome) = translate_memory_ingest(&command, &ExistingMemoryRefs::default())
            .expect("valid memory should translate");

        assert_eq!(update.root_node_id, "question:830ce83f");
        assert_eq!(update.role, "memory");
        assert_eq!(update.idempotency_key.as_deref(), Some("ingest:app-test"));
        assert_eq!(outcome.memory_id, "memory:app-test");
        assert_eq!(outcome.accepted.entries, 1);
        assert_eq!(outcome.accepted.relations, 1);
        assert_eq!(outcome.accepted.evidence, 1);
        assert_eq!(
            update
                .changes
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
        assert_eq!(
            update.changes[0].entity_id,
            "about:question:830ce83f:dimension:conversation:rachel-2026-04-12"
        );
        assert_eq!(
            update.changes[1].scopes,
            ["about:question:830ce83f:dimension:conversation:rachel-2026-04-12"]
        );
        assert_eq!(
            update.changes[2].entity_id,
            "relation:about:question:830ce83f:dimension:conversation:rachel-2026-04-12:contains_entry:claim:rachel-denver"
        );
        let entry_payload: serde_json::Value =
            serde_json::from_str(&update.changes[1].payload_json).expect("entry payload json");
        assert_eq!(
            entry_payload["coordinates"][0]["scope_id"],
            "about:question:830ce83f:dimension:conversation:rachel-2026-04-12"
        );
    }

    #[test]
    fn translate_memory_ingest_fails_fast_for_unknown_coordinate_dimension() {
        let mut command = sample_command();
        command.memory.entries[0].coordinates[0].scope_id = "conversation:missing".to_string();

        let error = translate_memory_ingest(&command, &ExistingMemoryRefs::default())
            .expect_err("unknown scope should fail");

        assert_validation_contains(error, "unknown dimension scope");
    }

    #[test]
    fn translate_memory_ingest_rejects_coordinate_kind_mismatch() {
        let mut command = sample_command();
        command.memory.entries[0].coordinates[0].dimension = "ceremony".to_string();

        let error = translate_memory_ingest(&command, &ExistingMemoryRefs::default())
            .expect_err("coordinate kind mismatch should fail");

        assert_validation_contains(error, "does not match declared kind `conversation`");
    }

    #[test]
    fn translate_memory_ingest_rejects_relation_coordinate_kind_mismatch() {
        let mut command = sample_command();
        let mut coordinate = command.memory.entries[0].coordinates[0].clone();
        coordinate.dimension = "ceremony".to_string();
        command.memory.relations[0].coordinate = Some(coordinate);

        let error = translate_memory_ingest(&command, &ExistingMemoryRefs::default())
            .expect_err("relation coordinate kind mismatch should fail");

        assert_validation_contains(error, "does not match declared kind `conversation`");
    }

    #[test]
    fn translate_memory_ingest_fails_fast_for_unknown_relation_endpoint() {
        let mut command = sample_command();
        command.memory.relations[0].target_ref = "claim:missing".to_string();

        let error = translate_memory_ingest(&command, &ExistingMemoryRefs::default())
            .expect_err("unknown ref should fail");

        assert_validation_contains(error, "references unknown refs");
    }

    #[test]
    fn translate_memory_ingest_canonicalizes_known_relation_types() {
        let mut command = sample_command();
        command.memory.relations[0].rel = " CONTAINS-ENTRY ".to_string();

        let (update, _) = translate_memory_ingest(&command, &ExistingMemoryRefs::default())
            .expect("known relation aliases should canonicalize");

        assert_eq!(
            update.changes[2].entity_id,
            "relation:about:question:830ce83f:dimension:conversation:rachel-2026-04-12:contains_entry:claim:rachel-denver"
        );
    }

    #[test]
    fn translate_memory_ingest_requires_non_structural_relation_proof() {
        let mut command = sample_command();
        command.memory.relations[0].semantic_class = "causal".to_string();
        command.memory.relations[0].why = None;
        command.memory.relations[0].evidence = None;
        command.memory.relations[0].confidence = None;

        let error = translate_memory_ingest(&command, &ExistingMemoryRefs::default())
            .expect_err("missing proof should fail");

        assert_validation_contains(error, "require confidence");
    }

    #[test]
    fn translate_memory_ingest_accepts_existing_materialized_refs() {
        let mut command = sample_command();
        command.memory.dimensions.clear();
        command.memory.entries[0].coordinates[0].scope_id = "conversation:existing".to_string();
        command.memory.relations[0].source_ref = "conversation:existing".to_string();
        command.memory.relations[0].target_ref = "claim:existing".to_string();
        command.memory.evidence[0].supports = vec!["claim:existing".to_string()];
        let existing = ExistingMemoryRefs {
            refs: [
                "conversation:existing".to_string(),
                "claim:existing".to_string(),
            ]
            .into_iter()
            .collect(),
            dimensions: ["conversation:existing".to_string()].into_iter().collect(),
        };

        let (update, outcome) =
            translate_memory_ingest(&command, &existing).expect("existing refs should validate");

        assert_eq!(outcome.accepted.entries, 1);
        assert_eq!(update.changes.len(), 3);
    }

    #[test]
    fn translate_memory_ingest_treats_existing_namespaced_dimension_as_idempotent() {
        let command = sample_command();
        let dimension_ref =
            "about:question:830ce83f:dimension:conversation:rachel-2026-04-12".to_string();
        let existing = ExistingMemoryRefs {
            refs: [dimension_ref.clone()].into_iter().collect(),
            dimensions: [dimension_ref.clone()].into_iter().collect(),
        };

        let (update, outcome) = translate_memory_ingest(&command, &existing)
            .expect("existing dimension declaration should be idempotent");

        assert_eq!(outcome.accepted.entries, 1);
        assert_eq!(
            update
                .changes
                .iter()
                .map(|change| change.entity_kind.as_str())
                .collect::<Vec<_>>(),
            vec!["memory_entry", "memory_relation", "memory_evidence"]
        );
        assert_eq!(
            update.changes[0].scopes,
            std::slice::from_ref(&dimension_ref)
        );
        assert_eq!(
            update.changes[1].entity_id,
            "relation:about:question:830ce83f:dimension:conversation:rachel-2026-04-12:contains_entry:claim:rachel-denver"
        );
    }

    #[test]
    fn translate_memory_ingest_keeps_existing_dimensions_as_known_relation_refs() {
        let mut command = sample_command();
        command.memory.dimensions.clear();
        let dimension_ref =
            "about:question:830ce83f:dimension:conversation:rachel-2026-04-12".to_string();
        command.memory.relations[0].source_ref = dimension_ref.clone();
        let existing = ExistingMemoryRefs {
            refs: BTreeSet::new(),
            dimensions: [dimension_ref].into_iter().collect(),
        };

        translate_memory_ingest(&command, &existing)
            .expect("existing dimensions should also be valid relation refs");
    }

    #[test]
    fn translate_memory_ingest_rejects_zero_coordinates_when_set() {
        let mut command = sample_command();
        command.memory.entries[0].coordinates[0].sequence = Some(0);

        let error = translate_memory_ingest(&command, &ExistingMemoryRefs::default())
            .expect_err("zero coordinate sequence should fail");

        assert_validation_contains(error, "sequence must be greater than zero");
    }

    fn sample_command() -> MemoryIngestCommand {
        MemoryIngestCommand {
            about: "question:830ce83f".to_string(),
            memory: MemoryData {
                dimensions: vec![MemoryDimensionData {
                    id: "conversation:rachel-2026-04-12".to_string(),
                    kind: "conversation".to_string(),
                    title: Some("Rachel relocation discussion".to_string()),
                    metadata: Default::default(),
                }],
                entries: vec![MemoryEntryData {
                    id: "claim:rachel-denver".to_string(),
                    kind: "claim".to_string(),
                    text: "Rachel said she was moving to Denver.".to_string(),
                    coordinates: vec![MemoryCoordinateData {
                        dimension: "conversation".to_string(),
                        scope_id: "conversation:rachel-2026-04-12".to_string(),
                        occurred_at: Some("2026-04-12T15:00:00Z".to_string()),
                        observed_at: None,
                        ingested_at: None,
                        valid_from: None,
                        valid_until: None,
                        sequence: Some(1),
                        rank: None,
                        metadata: Default::default(),
                    }],
                    metadata: Default::default(),
                }],
                relations: vec![MemoryRelationData {
                    source_ref: "conversation:rachel-2026-04-12".to_string(),
                    target_ref: "claim:rachel-denver".to_string(),
                    rel: "contains_entry".to_string(),
                    semantic_class: "structural".to_string(),
                    why: None,
                    evidence: None,
                    confidence: None,
                    sequence: Some(1),
                    motivation: None,
                    method: None,
                    decision_id: None,
                    caused_by_node_id: None,
                    coordinate: None,
                }],
                evidence: vec![MemoryEvidenceData {
                    id: "evidence:rachel-denver".to_string(),
                    supports: vec!["claim:rachel-denver".to_string()],
                    text: "Conversation transcript line 1".to_string(),
                    source: Some("transcript:1".to_string()),
                    time: Some("2026-04-12T15:00:00Z".to_string()),
                    metadata: Default::default(),
                }],
            },
            provenance: None,
            idempotency_key: "ingest:app-test".to_string(),
            dry_run: false,
        }
    }

    fn assert_validation_contains(error: ApplicationError, expected: &str) {
        match error {
            ApplicationError::Validation(message) => assert!(
                message.contains(expected),
                "expected `{message}` to contain `{expected}`"
            ),
            other => panic!("expected validation error, got {other:?}"),
        }
    }
}
