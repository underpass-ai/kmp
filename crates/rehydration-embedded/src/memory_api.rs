//! [`EmbeddedKernel`] as an implementation of the published consumer contract.
//!
//! The conversions in this module are the whole of the coupling a consumer is
//! allowed: domain aggregate in, plain view out. Nothing of the domain crosses
//! the trait.

use rehydration_application::{
    ApplicationError, AskMemoryQuery, GetContextResult, MemoryAnswerPolicy as DomainAnswerPolicy,
    WakeMemoryQuery,
};
use rehydration_domain::{DimensionSelection, ResolutionTier};
use rehydration_memory_api::{
    ApiCapabilities, ApiError, CONTRACT_VERSION, MemoryAnswerPolicy, MemoryAskRequest,
    MemoryDetailView, MemoryNodeView, MemoryRecallApi, MemoryRecallView, MemoryRelationshipView,
    MemoryTier, MemoryWakeRequest, RenderedMemoryView,
};

use crate::EmbeddedKernel;

/// What this build can do, by name.
///
/// Listed next to the implementation, so that adding a method to the trait
/// without adding its name is a diff a reviewer sees in one place.
const CAPABILITIES: [&str; 2] = ["wake", "ask"];

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
