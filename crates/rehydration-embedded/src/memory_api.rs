//! [`EmbeddedKernel`] as an implementation of the published consumer contract.
//!
//! The conversions in this module are the whole of the coupling a consumer is
//! allowed: domain aggregate in, plain view out. Nothing of the domain crosses
//! the trait.

use rehydration_application::{
    ApplicationError, AskMemoryQuery, GetContextResult, MemoryAnswerPolicy as DomainAnswerPolicy,
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData,
    MemoryIngestCommand, MemoryProvenanceData, WakeMemoryQuery,
};
use rehydration_domain::{DimensionSelection, PortError, ResolutionTier};
use rehydration_memory_api::{
    ApiCapabilities, ApiError, CONTRACT_VERSION, MemoryAnswerPolicy, MemoryAskRequest,
    MemoryDetailView, MemoryNodeView, MemoryRecallApi, MemoryRecallView, MemoryRecordApi,
    MemoryRecordRequest, MemoryRelationshipView, MemoryTier, MemoryWakeRequest, RecordedMemoryView,
    RenderedMemoryView,
};

use crate::EmbeddedKernel;

/// What this build can do, by name.
///
/// Listed next to the implementation, so that adding a method to the trait
/// without adding its name is a diff a reviewer sees in one place.
const CAPABILITIES: [&str; 3] = ["wake", "ask", "record"];

impl MemoryRecallApi for EmbeddedKernel {
    fn capabilities(&self) -> ApiCapabilities {
        ApiCapabilities::new(CONTRACT_VERSION, env!("CARGO_PKG_VERSION"), CAPABILITIES)
    }

    async fn wake(&self, request: MemoryWakeRequest) -> Result<MemoryRecallView, ApiError> {
        let about = request.about.clone();
        let query = WakeMemoryQuery {
            about: request.about,
            role: request.role,
            intent: request.intent,
            dimensions: dimensions(request.dimension_kinds, request.scoped_to_about),
            token_budget: request.token_budget,
            depth: request.depth,
            max_tier: request.max_tier.map(tier),
            max_entries: request.max_entries.map(|entries| entries as usize),
        };
        let result = self.service().wake(query).await.map_err(translate_error)?;
        Ok(recall_view(about, &result))
    }

    async fn ask(&self, request: MemoryAskRequest) -> Result<MemoryRecallView, ApiError> {
        let about = request.about.clone();
        let query = AskMemoryQuery {
            about: request.about,
            question: request.question,
            answer_policy: answer_policy(request.answer_policy),
            dimensions: dimensions(request.dimension_kinds, request.scoped_to_about),
            token_budget: request.token_budget,
            depth: request.depth,
            max_tier: request.max_tier.map(tier),
        };
        let result = self.service().ask(query).await.map_err(translate_error)?;
        Ok(recall_view(about, &result))
    }
}

impl MemoryRecordApi for EmbeddedKernel {
    fn capabilities(&self) -> ApiCapabilities {
        ApiCapabilities::new(CONTRACT_VERSION, env!("CARGO_PKG_VERSION"), CAPABILITIES)
    }

    async fn record(&self, request: MemoryRecordRequest) -> Result<RecordedMemoryView, ApiError> {
        let outcome = self
            .service()
            .ingest(ingest_command(request))
            .await
            .map_err(translate_record_error)?;
        Ok(RecordedMemoryView {
            about: outcome.about,
            memory_id: outcome.memory_id,
            accepted_entries: outcome.accepted.entries,
            accepted_relations: outcome.accepted.relations,
            accepted_evidence: outcome.accepted.evidence,
            read_after_write_ready: outcome.read_after_write_ready,
            warnings: outcome.warnings,
        })
    }
}

fn ingest_command(request: MemoryRecordRequest) -> MemoryIngestCommand {
    MemoryIngestCommand {
        about: request.about,
        memory: MemoryData {
            dimensions: request
                .dimensions
                .into_iter()
                .map(|dimension| MemoryDimensionData {
                    id: dimension.id,
                    kind: dimension.kind,
                    title: dimension.title,
                    metadata: dimension.metadata,
                })
                .collect(),
            entries: request
                .entries
                .into_iter()
                .map(|entry| MemoryEntryData {
                    id: entry.id,
                    kind: entry.kind,
                    text: entry.text,
                    coordinates: entry
                        .coordinates
                        .into_iter()
                        .map(|coordinate| MemoryCoordinateData {
                            dimension: coordinate.dimension,
                            scope_id: coordinate.scope_id,
                            occurred_at: coordinate.occurred_at,
                            observed_at: None,
                            ingested_at: None,
                            valid_from: None,
                            valid_until: None,
                            sequence: coordinate.sequence,
                            rank: coordinate.rank,
                            metadata: Default::default(),
                        })
                        .collect(),
                    metadata: entry.metadata,
                })
                .collect(),
            relations: request
                .relations
                .into_iter()
                .map(|relation| rehydration_application::MemoryRelationData {
                    source_ref: relation.from,
                    target_ref: relation.to,
                    rel: relation.rel,
                    semantic_class: relation.semantic_class,
                    why: relation.why,
                    evidence: None,
                    confidence: relation.confidence,
                    sequence: relation.sequence,
                    motivation: None,
                    method: None,
                    decision_id: None,
                    caused_by_node_id: None,
                    coordinate: None,
                })
                .collect(),
            evidence: request
                .evidence
                .into_iter()
                .map(|evidence| MemoryEvidenceData {
                    id: evidence.id,
                    supports: evidence.supports,
                    text: evidence.text,
                    source: evidence.source,
                    time: evidence.time,
                    metadata: evidence.metadata,
                })
                .collect(),
        },
        provenance: request.provenance.map(|provenance| MemoryProvenanceData {
            source_kind: provenance.source_kind,
            source_agent: provenance.source_agent,
            observed_at: provenance.observed_at,
            correlation_id: provenance.correlation_id,
            causation_id: provenance.causation_id,
        }),
        idempotency_key: request.idempotency_key,
        dry_run: false,
    }
}

/// The record side's error map.
///
/// One case more than the recall side's: a port `Conflict` here is the
/// idempotency key reused with different content — the kernel looked at the
/// request and said no, and retrying it unchanged earns the same answer. On
/// the recall side a conflict cannot mean that, so the shared map's reading
/// of ports-as-environment stays right there.
fn translate_record_error(error: ApplicationError) -> ApiError {
    match error {
        ApplicationError::Ports(PortError::Conflict(reason)) => ApiError::Refused { reason },
        other => translate_error(other),
    }
}

fn dimensions(kinds: Vec<String>, scoped_to_about: bool) -> DimensionSelection {
    let selection = if kinds.is_empty() {
        DimensionSelection::all()
    } else {
        DimensionSelection::only(kinds)
    };
    if scoped_to_about {
        selection.with_current_about_scope()
    } else {
        selection
    }
}

fn tier(tier: MemoryTier) -> ResolutionTier {
    match tier {
        MemoryTier::Summary => ResolutionTier::L0Summary,
        MemoryTier::CausalSpine => ResolutionTier::L1CausalSpine,
        MemoryTier::EvidencePack => ResolutionTier::L2EvidencePack,
    }
}

fn answer_policy(policy: MemoryAnswerPolicy) -> DomainAnswerPolicy {
    match policy {
        MemoryAnswerPolicy::EvidenceOrUnknown => DomainAnswerPolicy::EvidenceOrUnknown,
        MemoryAnswerPolicy::ShowConflicts => DomainAnswerPolicy::ShowConflicts,
        MemoryAnswerPolicy::BestEffort => DomainAnswerPolicy::BestEffort,
    }
}

fn recall_view(about: String, result: &GetContextResult) -> MemoryRecallView {
    let bundle = &result.bundle;
    MemoryRecallView {
        about,
        revision: bundle.metadata().revision,
        content_hash: bundle.metadata().content_hash.clone(),
        root: node_view(bundle.root_node()),
        neighbors: bundle.neighbor_nodes().iter().map(node_view).collect(),
        relationships: bundle
            .relationships()
            .iter()
            .map(|relationship| MemoryRelationshipView {
                source_node_id: relationship.source_node_id().to_string(),
                target_node_id: relationship.target_node_id().to_string(),
                relationship_type: relationship.relationship_type().to_string(),
            })
            .collect(),
        details: bundle
            .node_details()
            .iter()
            .map(|detail| MemoryDetailView {
                node_id: detail.node_id().to_string(),
                detail: detail.detail().to_string(),
                content_hash: detail.content_hash().to_string(),
                revision: detail.revision(),
            })
            .collect(),
        rendered: RenderedMemoryView {
            content: result.rendered.content.clone(),
            content_hash: result.rendered.content_hash.clone(),
            token_count: result.rendered.token_count,
            quality: rehydration_memory_api::MemoryQualityView {
                raw_equivalent_tokens: result.rendered.quality.raw_equivalent_tokens(),
                compression_ratio: result.rendered.quality.compression_ratio(),
                causal_density: result.rendered.quality.causal_density(),
                noise_ratio: result.rendered.quality.noise_ratio(),
                detail_coverage: result.rendered.quality.detail_coverage(),
            },
        },
    }
}

fn node_view(node: &rehydration_domain::BundleNode) -> MemoryNodeView {
    MemoryNodeView {
        node_id: node.node_id().to_string(),
        node_kind: node.node_kind().to_string(),
        title: node.title().to_string(),
        summary: node.summary().to_string(),
        status: node.status().to_string(),
        labels: node.labels().to_vec(),
        properties: node.properties().clone(),
    }
}

fn translate_error(error: ApplicationError) -> ApiError {
    match error {
        ApplicationError::NotFound(what) => ApiError::NotFound { what },
        ApplicationError::Validation(reason) => ApiError::Refused { reason },
        refused @ ApplicationError::Domain(_) => ApiError::Refused {
            reason: refused.to_string(),
        },
        // A port failing is the storage or the environment, not the request.
        unavailable @ ApplicationError::Ports(_) => ApiError::Unavailable {
            reason: unavailable.to_string(),
        },
    }
}
