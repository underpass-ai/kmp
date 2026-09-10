use kmp_application::{
    LabelPolicy, MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData,
    MemoryEvidenceData, MemoryIngestCommand, MemoryIngestOutcome, MemoryProvenanceData,
    MemoryRelationData,
};
use kmp_proto::v1beta1::{
    AcceptedCounts, IngestRequest, IngestResponse, IngestedMemory, LabelPolicy as ProtoLabelPolicy,
    MemoryDimension, MemoryEvidence, MemoryProvenance, MemoryRelation, ResemblingLabel,
    TemporalCoordinate as ProtoTemporalCoordinate,
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

    let receipt_context = request
        .receipt_context_json
        .map(|text| {
            let value: serde_json::Value = serde_json::from_str(&text)
                .map_err(|_| invalid_argument("receipt_context_json must encode an object"))?;
            if !value.is_object() {
                return Err(invalid_argument(
                    "receipt_context_json must encode an object",
                ));
            }
            Ok(value)
        })
        .transpose()?;

    Ok(MemoryIngestCommand {
        receipt_context,
        default_observation_to_ingestion: request.default_observation_to_ingestion,
        neighborhood_review: request.neighborhood_review,
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
        label_policy: match ProtoLabelPolicy::try_from(request.label_policy) {
            Ok(ProtoLabelPolicy::Refuse) => LabelPolicy::Refuse,
            _ => LabelPolicy::Warn,
        },
    })
}

pub fn ingest_response_from_outcome(outcome: MemoryIngestOutcome) -> IngestResponse {
    IngestResponse {
        neighborhood: outcome
            .neighborhood
            .map(|view| kmp_proto::v1beta1::WriteNeighborhood {
                token: view.token,
                eligible: view.eligible as u32,
                omitted: view.omitted as u32,
                omitted_conflicts: view.omitted_conflicts as u32,
                abouts: view.abouts,
                partial: view.partial,
                links: view
                    .links
                    .into_iter()
                    .map(|link| kmp_proto::v1beta1::NeighborhoodLink {
                        from: link.from as u32,
                        rel: link.rel,
                        to: link.to as u32,
                    })
                    .collect(),
                items: view
                    .items
                    .into_iter()
                    .map(|item| kmp_proto::v1beta1::NeighborhoodItem {
                        about: item.about,
                        r#ref: item.reference,
                        state: item.state,
                        kind: item.kind,
                        reason: item.reason,
                        text: item.text,
                        text_omitted: item.text_omitted,
                        clocks: item
                            .clocks
                            .into_iter()
                            .map(|(axis, values)| kmp_proto::v1beta1::NeighborhoodClock {
                                axis,
                                values,
                            })
                            .collect(),
                    })
                    .collect(),
            }),
        summary: format!(
            "{} {} {}, {} {}, and {} {} for {}.",
            if !outcome.read_after_write_ready {
                "Validated"
            } else if outcome.replayed {
                "Replayed"
            } else {
                "Ingested"
            },
            outcome.accepted.entries,
            plural(outcome.accepted.entries, "entry", "entries"),
            outcome.accepted.relations,
            plural(outcome.accepted.relations, "relation", "relations"),
            outcome.accepted.evidence,
            plural(outcome.accepted.evidence, "evidence item", "evidence items"),
            outcome.about
        ),
        memory: Some(IngestedMemory {
            receipt_ref: outcome.receipt_ref,
            replayed: outcome.replayed,
            clocks: outcome.clocks.map(write_clocks_to_proto),
            about: outcome.about,
            memory_id: outcome.memory_id,
            accepted: Some(AcceptedCounts {
                entries: outcome.accepted.entries as u32,
                relations: outcome.accepted.relations as u32,
                evidence: outcome.accepted.evidence as u32,
            }),
            read_after_write_ready: outcome.read_after_write_ready,
            created_dimensions: outcome.created_dimensions,
            resembling_labels: outcome
                .resembling_labels
                .into_iter()
                .map(|label| ResemblingLabel {
                    key: label.key,
                    value: label.value,
                    existing_key: label.existing_key,
                    existing_value: label.existing_value,
                    kind: label.kind,
                    why: label.why,
                })
                .collect(),
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

fn entry_from_proto(value: kmp_proto::v1beta1::MemoryEntry) -> MemoryEntryData {
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

pub(super) fn provenance_from_proto(value: MemoryProvenance) -> MemoryProvenanceData {
    MemoryProvenanceData {
        source_kind: source_kind_name(value.source_kind()),
        source_agent: value.source_agent,
        observed_at: proto_timestamp_to_sort_string(value.observed_at),
        correlation_id: non_empty(value.correlation_id),
        causation_id: non_empty(value.causation_id),
    }
}

fn write_clocks_to_proto(
    value: kmp_application::memory::WriteClocks,
) -> kmp_proto::v1beta1::WriteClocks {
    use kmp_proto::v1beta1::WriteClocks;
    WriteClocks {
        entries: value.entries as u32,
        occurred: Some(write_clock_to_proto(value.occurred)),
        observed: Some(write_clock_to_proto(value.observed)),
        ingested: Some(write_clock_to_proto(value.ingested)),
        valid_from: Some(write_clock_to_proto(value.valid_from)),
        valid_until: Some(write_clock_to_proto(value.valid_until)),
    }
}

fn write_clock_to_proto(
    value: kmp_application::memory::WriteClockCoverage,
) -> kmp_proto::v1beta1::WriteClockCoverage {
    kmp_proto::v1beta1::WriteClockCoverage {
        entries: value.entries as u32,
        distinct_values: value.distinct_values as u32,
        single_value: super::scalars::timestamp_from_sort_or_rfc3339(value.single_value.as_deref()),
    }
}
