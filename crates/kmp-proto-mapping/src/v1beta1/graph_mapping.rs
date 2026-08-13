use kmp_application::{GraphNodeView, NodeDetailView};
use kmp_domain::{
    BundleNode, BundleNodeDetail, BundleRelationship, Provenance, RelationExplanation,
    RelationSemanticClass, SourceKind,
};
use kmp_proto::v1beta1::{
    BundleNodeDetail as ProtoBundleNodeDetail, GraphNode, GraphRelationship,
    GraphRelationshipExplanation, GraphRelationshipSemanticClass, Provenance as ProtoProvenance,
};

pub fn proto_graph_node_v1beta1(node: &GraphNodeView) -> GraphNode {
    GraphNode {
        node_id: node.node_id.clone(),
        node_kind: node.node_kind.clone(),
        title: node.title.clone(),
        summary: node.summary.clone(),
        status: node.status.clone(),
        labels: node.labels.clone(),
        properties: node.properties.clone().into_iter().collect(),
        provenance: None, // GraphNodeView does not carry provenance yet
    }
}

pub fn proto_bundle_node_v1beta1(node: &BundleNode) -> GraphNode {
    GraphNode {
        node_id: node.node_id().to_string(),
        node_kind: node.node_kind().to_string(),
        title: node.title().to_string(),
        summary: node.summary().to_string(),
        status: node.status().to_string(),
        labels: node.labels().to_vec(),
        properties: node.properties().clone().into_iter().collect(),
        provenance: node.provenance().map(proto_provenance_v1beta1),
    }
}

fn proto_provenance_v1beta1(p: &Provenance) -> ProtoProvenance {
    ProtoProvenance {
        source_kind: proto_source_kind(p.source_kind()) as i32,
        source_agent: p.source_agent().unwrap_or_default().to_string(),
        observed_at: p.observed_at().unwrap_or_default().to_string(),
    }
}

fn proto_source_kind(kind: &SourceKind) -> kmp_proto::v1beta1::SourceKind {
    match kind {
        SourceKind::Human => kmp_proto::v1beta1::SourceKind::Human,
        SourceKind::Agent => kmp_proto::v1beta1::SourceKind::Agent,
        SourceKind::Projection => kmp_proto::v1beta1::SourceKind::Projection,
        SourceKind::Derived => kmp_proto::v1beta1::SourceKind::Derived,
        SourceKind::Unknown => kmp_proto::v1beta1::SourceKind::Unspecified,
    }
}

pub fn proto_bundle_relationship_v1beta1(relationship: &BundleRelationship) -> GraphRelationship {
    GraphRelationship {
        source_node_id: relationship.source_node_id().to_string(),
        target_node_id: relationship.target_node_id().to_string(),
        relationship_type: relationship.relationship_type().to_string(),
        explanation: Some(proto_graph_relationship_explanation_v1beta1(
            relationship.explanation(),
        )),
        provenance: relationship.provenance().map(proto_provenance_v1beta1),
    }
}

pub fn proto_bundle_node_detail_v1beta1(detail: &BundleNodeDetail) -> ProtoBundleNodeDetail {
    ProtoBundleNodeDetail {
        node_id: detail.node_id().to_string(),
        detail: detail.detail().to_string(),
        content_hash: detail.content_hash().to_string(),
        revision: detail.revision(),
    }
}

pub fn proto_node_detail_view_v1beta1(detail: &NodeDetailView) -> ProtoBundleNodeDetail {
    ProtoBundleNodeDetail {
        node_id: detail.node_id.clone(),
        detail: detail.detail.clone(),
        content_hash: detail.content_hash.clone(),
        revision: detail.revision,
    }
}

fn proto_graph_relationship_explanation_v1beta1(
    explanation: &RelationExplanation,
) -> GraphRelationshipExplanation {
    GraphRelationshipExplanation {
        semantic_class: proto_graph_relationship_semantic_class_v1beta1(
            explanation.semantic_class(),
        ) as i32,
        rationale: explanation.rationale().unwrap_or_default().to_string(),
        motivation: explanation.motivation().unwrap_or_default().to_string(),
        method: explanation.method().unwrap_or_default().to_string(),
        decision_id: explanation.decision_id().unwrap_or_default().to_string(),
        caused_by_node_id: explanation
            .caused_by_node_id()
            .unwrap_or_default()
            .to_string(),
        evidence: explanation.evidence().unwrap_or_default().to_string(),
        confidence: explanation.confidence().unwrap_or_default().to_string(),
        sequence: explanation.sequence().unwrap_or_default(),
        dimension: explanation.dimension().unwrap_or_default().to_string(),
        scope_id: explanation.scope_id().unwrap_or_default().to_string(),
        occurred_at: explanation.occurred_at().unwrap_or_default().to_string(),
        observed_at: explanation.observed_at().unwrap_or_default().to_string(),
        ingested_at: explanation.ingested_at().unwrap_or_default().to_string(),
        valid_from: explanation.valid_from().unwrap_or_default().to_string(),
        valid_until: explanation.valid_until().unwrap_or_default().to_string(),
        rank: explanation.rank().unwrap_or_default(),
    }
}

fn proto_graph_relationship_semantic_class_v1beta1(
    semantic_class: &RelationSemanticClass,
) -> GraphRelationshipSemanticClass {
    match semantic_class {
        RelationSemanticClass::Structural => GraphRelationshipSemanticClass::Structural,
        RelationSemanticClass::Causal => GraphRelationshipSemanticClass::Causal,
        RelationSemanticClass::Motivational => GraphRelationshipSemanticClass::Motivational,
        RelationSemanticClass::Procedural => GraphRelationshipSemanticClass::Procedural,
        RelationSemanticClass::Evidential => GraphRelationshipSemanticClass::Evidential,
        RelationSemanticClass::Constraint => GraphRelationshipSemanticClass::Constraint,
    }
}
