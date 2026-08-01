use rehydration_application::{
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData,
    MemoryIngestCommand, MemoryIngestOutcome, MemoryProvenanceData, MemoryRelationData,
};
use rehydration_proto::v1beta1::{
    AcceptedCounts, IngestRequest, IngestResponse, IngestedMemory, MemoryDimension, MemoryEvidence,
    MemoryProvenance, MemoryRelation, TemporalCoordinate as ProtoTemporalCoordinate,
};

use super::scalars::{
    ProtoMappingResult, confidence_name, invalid_argument, non_empty, plural,
    proto_timestamp_to_sort_string, semantic_class_name, source_kind_name,
};

pub fn ingest_command_from_proto(
    request: IngestRequest,
) -> ProtoMappingResult<MemoryIngestCommand> {
    let memory = request
        .memory
        .ok_or_else(|| invalid_argument("memory is required"))?;

    Ok(MemoryIngestCommand {
        about: request.about,
        memory: MemoryData {
            dimensions: memory
                .dimensions
                .into_iter()
                .map(dimension_from_proto)
                .collect(),
            entries: memory.entries.into_iter().map(entry_from_proto).collect(),
            relations: memory
                .relations
                .into_iter()
                .map(relation_from_proto)
                .collect(),
            evidence: memory
                .evidence
                .into_iter()
                .map(evidence_from_proto)
                .collect(),
        },
        provenance: request.provenance.map(provenance_from_proto),
        idempotency_key: request.idempotency_key,
        dry_run: request.dry_run,
    })
}

pub fn ingest_response_from_outcome(outcome: MemoryIngestOutcome) -> IngestResponse {
    IngestResponse {
        summary: format!(
            "Ingested {} {}, {} {}, and {} {} for {}.",
            outcome.accepted.entries,
            plural(outcome.accepted.entries, "entry", "entries"),
            outcome.accepted.relations,
            plural(outcome.accepted.relations, "relation", "relations"),
            outcome.accepted.evidence,
            plural(outcome.accepted.evidence, "evidence item", "evidence items"),
            outcome.about
        ),
        memory: Some(IngestedMemory {
            about: outcome.about,
            memory_id: outcome.memory_id,
            accepted: Some(AcceptedCounts {
                entries: outcome.accepted.entries as u32,
                relations: outcome.accepted.relations as u32,
                evidence: outcome.accepted.evidence as u32,
            }),
            read_after_write_ready: outcome.read_after_write_ready,
        }),
        warnings: outcome.warnings,
    }
}

fn dimension_from_proto(value: MemoryDimension) -> MemoryDimensionData {
    MemoryDimensionData {
        id: value.id,
        kind: value.kind,
        title: non_empty(value.title),
        metadata: value.metadata.into_iter().collect(),
    }
}

fn entry_from_proto(value: rehydration_proto::v1beta1::MemoryEntry) -> MemoryEntryData {
    MemoryEntryData {
        id: value.id,
        kind: value.kind,
        text: value.text,
        coordinates: value
            .coordinates
            .into_iter()
            .map(coordinate_from_proto)
            .collect(),
        metadata: value.metadata.into_iter().collect(),
    }
}

fn coordinate_from_proto(value: ProtoTemporalCoordinate) -> MemoryCoordinateData {
    MemoryCoordinateData {
        dimension: value.dimension,
        scope_id: value.scope_id,
        occurred_at: proto_timestamp_to_sort_string(value.occurred_at),
        observed_at: proto_timestamp_to_sort_string(value.observed_at),
        ingested_at: proto_timestamp_to_sort_string(value.ingested_at),
        valid_from: proto_timestamp_to_sort_string(value.valid_from),
        valid_until: proto_timestamp_to_sort_string(value.valid_until),
        sequence: value.sequence,
        rank: value.rank,
        metadata: value.metadata.into_iter().collect(),
    }
}

fn relation_from_proto(value: MemoryRelation) -> MemoryRelationData {
    let semantic_class = semantic_class_name(value.semantic_class());
    let confidence = confidence_name(value.confidence());
    let explanation = value.explanation.unwrap_or_default();

    MemoryRelationData {
        source_ref: value.source_ref,
        target_ref: value.target_ref,
        rel: value.rel,
        semantic_class,
        why: non_empty(value.why),
        evidence: non_empty(value.evidence),
        confidence,
        sequence: value.sequence,
        motivation: non_empty(explanation.motivation),
        method: non_empty(explanation.method),
        decision_id: non_empty(explanation.decision_id),
        caused_by_node_id: non_empty(explanation.caused_by_node_id),
        coordinate: explanation.coordinate.map(coordinate_from_proto),
    }
}

fn evidence_from_proto(value: MemoryEvidence) -> MemoryEvidenceData {
    MemoryEvidenceData {
        id: value.id,
        supports: value.supports,
        text: value.text,
        source: non_empty(value.source),
        time: proto_timestamp_to_sort_string(value.time),
        metadata: value.metadata.into_iter().collect(),
    }
}

fn provenance_from_proto(value: MemoryProvenance) -> MemoryProvenanceData {
    MemoryProvenanceData {
        source_kind: source_kind_name(value.source_kind()),
        source_agent: value.source_agent,
        observed_at: proto_timestamp_to_sort_string(value.observed_at)
            .unwrap_or_else(|| "unix:100000000000:000000000".to_string()),
        correlation_id: non_empty(value.correlation_id),
        causation_id: non_empty(value.causation_id),
    }
}
