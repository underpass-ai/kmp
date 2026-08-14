use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::repositories::PortError;
use crate::value_objects::{RelationExplanation, RelationSemanticClass};

use super::ProjectionCheckpoint;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionEnvelope {
    pub event_id: String,
    pub correlation_id: String,
    pub causation_id: String,
    pub occurred_at: String,
    pub aggregate_id: String,
    pub aggregate_type: String,
    pub schema_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedNodeReference {
    pub node_id: String,
    pub relation_type: String,
    pub explanation: RelatedNodeExplanationData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedNodeExplanationData {
    pub semantic_class: RelationSemanticClass,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub motivation: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub decision_id: Option<String>,
    #[serde(default)]
    pub caused_by_node_id: Option<String>,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub confidence: Option<String>,
    #[serde(default)]
    pub sequence: Option<u32>,
}

impl TryFrom<RelatedNodeExplanationData> for RelationExplanation {
    type Error = crate::DomainError;

    fn try_from(value: RelatedNodeExplanationData) -> Result<Self, Self::Error> {
        Ok(RelationExplanation::new(value.semantic_class)
            .with_optional_rationale(value.rationale)
            .with_optional_motivation(value.motivation)
            .with_optional_method(value.method)
            .with_optional_decision_id(value.decision_id)
            .with_optional_caused_by_node_id(value.caused_by_node_id)
            .with_optional_evidence(value.evidence)
            .with_optional_confidence(value.confidence)
            .with_optional_sequence(value.sequence))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNodeMaterializedData {
    pub node_id: String,
    pub node_kind: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub labels: Vec<String>,
    pub properties: BTreeMap<String, String>,
    pub related_nodes: Vec<RelatedNodeReference>,
    /// Provenance: who produced this node and when.
    #[serde(default)]
    pub source_kind: Option<String>,
    #[serde(default)]
    pub source_agent: Option<String>,
    #[serde(default)]
    pub observed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNodeMaterializedEvent {
    #[serde(flatten)]
    pub envelope: ProjectionEnvelope,
    pub data: GraphNodeMaterializedData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphRelationMaterializedData {
    pub source_node_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub explanation: RelatedNodeExplanationData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphRelationMaterializedEvent {
    #[serde(flatten)]
    pub envelope: ProjectionEnvelope,
    pub data: GraphRelationMaterializedData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeDetailMaterializedData {
    pub node_id: String,
    pub detail: String,
    pub content_hash: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeDetailMaterializedEvent {
    #[serde(flatten)]
    pub envelope: ProjectionEnvelope,
    pub data: NodeDetailMaterializedData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionEvent {
    GraphNodeMaterialized(GraphNodeMaterializedEvent),
    GraphRelationMaterialized(GraphRelationMaterializedEvent),
    NodeDetailMaterialized(NodeDetailMaterializedEvent),
}

impl ProjectionEvent {
    pub fn event_id(&self) -> &str {
        match self {
            Self::GraphNodeMaterialized(event) => &event.envelope.event_id,
            Self::GraphRelationMaterialized(event) => &event.envelope.event_id,
            Self::NodeDetailMaterialized(event) => &event.envelope.event_id,
        }
    }

    pub fn envelope(&self) -> &ProjectionEnvelope {
        match self {
            Self::GraphNodeMaterialized(event) => &event.envelope,
            Self::GraphRelationMaterialized(event) => &event.envelope,
            Self::NodeDetailMaterialized(event) => &event.envelope,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionHandlingRequest {
    pub consumer_name: String,
    pub stream_name: String,
    pub subject: String,
    pub event: ProjectionEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionHandlingResult {
    pub event_id: String,
    pub subject: String,
    pub duplicate: bool,
    pub applied_mutations: usize,
    pub checkpoint: Option<ProjectionCheckpoint>,
}

pub trait ProjectionEventHandler {
    fn handle_projection_event(
        &self,
        request: ProjectionHandlingRequest,
    ) -> impl std::future::Future<Output = Result<ProjectionHandlingResult, PortError>> + Send;
}
