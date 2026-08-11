//! The viewer's wire format: plain serde views over application results.
//!
//! Projections, never the aggregates — the kernel's domain types gain fields
//! as the domain needs them, and a browser client that read them directly
//! would inherit each one as a contract. Everything here is what the UI
//! renders, nothing more.

use std::collections::BTreeMap;

use rehydration_application::{
    GetContextPathResult, GetContextResult, GetNodeDetailResult, GraphNodeView,
    GraphRelationshipView, InspectMemoryResult, RenderedContext, TemporalMemoryResult,
};
use rehydration_domain::{
    BundleNode, BundleQualityMetrics, BundleRelationship, RelationExplanation, TemporalCoordinate,
    TemporalDirection,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct NodeView {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub labels: Vec<String>,
    pub properties: BTreeMap<String, String>,
}

impl NodeView {
    fn from_bundle_node(node: &BundleNode) -> Self {
        Self {
            id: node.node_id().to_string(),
            kind: node.node_kind().to_string(),
            title: node.title().to_string(),
            summary: node.summary().to_string(),
            status: node.status().to_string(),
            labels: node.labels().to_vec(),
            properties: node.properties().clone(),
        }
    }

    pub fn from_graph_node(node: &GraphNodeView) -> Self {
        Self {
            id: node.node_id.clone(),
            kind: node.node_kind.clone(),
            title: node.title.clone(),
            summary: node.summary.clone(),
            status: node.status.clone(),
            labels: node.labels.clone(),
            properties: node.properties.clone(),
        }
    }
}

/// One typed relation, with the explanation the recorder gave. The UI quotes
/// `why` and `evidence` instead of inventing a rationale for the link.
#[derive(Debug, Clone, Serialize)]
pub struct EdgeView {
    pub source: String,
    pub target: String,
    pub rel: String,
    pub class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motivation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caused_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u32>,
}

impl EdgeView {
    fn new(source: &str, target: &str, rel: &str, explanation: &RelationExplanation) -> Self {
        Self {
            source: source.to_string(),
            target: target.to_string(),
            rel: rel.to_string(),
            class: explanation.semantic_class().as_str().to_string(),
            why: explanation.rationale().map(ToString::to_string),
            evidence: explanation.evidence().map(ToString::to_string),
            confidence: explanation.confidence().map(ToString::to_string),
            motivation: explanation.motivation().map(ToString::to_string),
            method: explanation.method().map(ToString::to_string),
            decision_id: explanation.decision_id().map(ToString::to_string),
            caused_by: explanation.caused_by_node_id().map(ToString::to_string),
            dimension: explanation.dimension().map(ToString::to_string),
            scope_id: explanation.scope_id().map(ToString::to_string),
            occurred_at: explanation.occurred_at().map(ToString::to_string),
            sequence: explanation.sequence(),
        }
    }

    fn from_bundle_relationship(relationship: &BundleRelationship) -> Self {
        Self::new(
            relationship.source_node_id(),
            relationship.target_node_id(),
            relationship.relationship_type(),
            relationship.explanation(),
        )
    }

    pub fn from_graph_relationship(relationship: &GraphRelationshipView) -> Self {
        Self::new(
            &relationship.source_node_id,
            &relationship.target_node_id,
            &relationship.relationship_type,
            &relationship.explanation,
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DetailView {
    pub id: String,
    pub detail: String,
    pub content_hash: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderedView {
    pub content: String,
    pub content_hash: String,
    pub token_count: u32,
}

impl RenderedView {
    fn from_rendered(rendered: &RenderedContext) -> Self {
        Self {
            content: rendered.content.clone(),
            content_hash: rendered.content_hash.clone(),
            token_count: rendered.token_count,
        }
    }
}

/// The kernel's own account of the trade a render made.
#[derive(Debug, Clone, Serialize)]
pub struct QualityView {
    pub raw_equivalent_tokens: u32,
    pub compression_ratio: f64,
    pub causal_density: f64,
    pub noise_ratio: f64,
    pub detail_coverage: f64,
}

impl QualityView {
    fn from_metrics(quality: &BundleQualityMetrics) -> Self {
        Self {
            raw_equivalent_tokens: quality.raw_equivalent_tokens(),
            compression_ratio: quality.compression_ratio(),
            causal_density: quality.causal_density(),
            noise_ratio: quality.noise_ratio(),
            detail_coverage: quality.detail_coverage(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CoordinateView {
    pub dimension: String,
    pub scope_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingested_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<String>,
}

impl CoordinateView {
    fn from_coordinate(coordinate: &TemporalCoordinate) -> Self {
        Self {
            dimension: coordinate.dimension().to_string(),
            scope_id: coordinate.scope_id().to_string(),
            sequence: coordinate.sequence(),
            rank: coordinate.rank(),
            occurred_at: coordinate.occurred_at().map(ToString::to_string),
            observed_at: coordinate.observed_at().map(ToString::to_string),
            ingested_at: coordinate.ingested_at().map(ToString::to_string),
            valid_from: coordinate.valid_from().map(ToString::to_string),
            valid_until: coordinate.valid_until().map(ToString::to_string),
        }
    }
}

/// One recalled neighborhood, ready to draw: nodes, typed edges, details, and
/// the kernel's rendered context with its quality account.
#[derive(Debug, Clone, Serialize)]
pub struct GraphView {
    pub about: String,
    pub root_id: String,
    pub revision: u64,
    pub content_hash: String,
    pub nodes: Vec<NodeView>,
    pub edges: Vec<EdgeView>,
    pub details: Vec<DetailView>,
    pub rendered: RenderedView,
    pub quality: QualityView,
}

pub fn graph_view(about: &str, result: &GetContextResult) -> GraphView {
    let bundle = &result.bundle;
    let mut nodes = Vec::with_capacity(1 + bundle.neighbor_nodes().len());
    nodes.push(NodeView::from_bundle_node(bundle.root_node()));
    nodes.extend(
        bundle
            .neighbor_nodes()
            .iter()
            .map(NodeView::from_bundle_node),
    );
    GraphView {
        about: about.to_string(),
        root_id: bundle.root_node_id().as_str().to_string(),
        revision: bundle.metadata().revision,
        content_hash: bundle.metadata().content_hash.clone(),
        nodes,
        edges: bundle
            .relationships()
            .iter()
            .map(EdgeView::from_bundle_relationship)
            .collect(),
        details: bundle
            .node_details()
            .iter()
            .map(|detail| DetailView {
                id: detail.node_id().to_string(),
                detail: detail.detail().to_string(),
                content_hash: detail.content_hash().to_string(),
                revision: detail.revision(),
            })
            .collect(),
        rendered: RenderedView::from_rendered(&result.rendered),
        quality: QualityView::from_metrics(&result.rendered.quality),
    }
}

/// One node in full: summary card, detail text, and its typed links both ways.
#[derive(Debug, Clone, Serialize)]
pub struct NodeInspectView {
    pub node: NodeView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<DetailView>,
    pub incoming: Vec<EdgeView>,
    pub outgoing: Vec<EdgeView>,
    pub raw_coordinates: Vec<CoordinateView>,
}

pub fn node_inspect_view(result: &InspectMemoryResult) -> NodeInspectView {
    NodeInspectView {
        node: NodeView::from_graph_node(&result.detail.node),
        detail: detail_view(&result.detail),
        incoming: result
            .incoming
            .iter()
            .map(EdgeView::from_graph_relationship)
            .collect(),
        outgoing: result
            .outgoing
            .iter()
            .map(EdgeView::from_graph_relationship)
            .collect(),
        raw_coordinates: result
            .raw_coordinates
            .iter()
            .map(CoordinateView::from_coordinate)
            .collect(),
    }
}

fn detail_view(result: &GetNodeDetailResult) -> Option<DetailView> {
    result.detail.as_ref().map(|detail| DetailView {
        id: detail.node_id.clone(),
        detail: detail.detail.clone(),
        content_hash: detail.content_hash.clone(),
        revision: detail.revision,
    })
}

/// Batch of node summaries; ids the kernel does not know go to `missing`
/// instead of failing the ones it does.
#[derive(Debug, Clone, Serialize)]
pub struct NodeBatchView {
    pub nodes: Vec<NodeView>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEntryView {
    pub ref_id: String,
    pub kind: String,
    pub text: String,
    pub coordinates: Vec<CoordinateView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelinePageView {
    pub returned: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineView {
    pub about: String,
    pub direction: String,
    pub resolved_cursor: CoordinateView,
    pub included_dimensions: Vec<String>,
    pub missing_dimensions: Vec<String>,
    pub entries: Vec<TimelineEntryView>,
    pub page: TimelinePageView,
    pub missing: Vec<String>,
}

pub fn timeline_view(about: &str, result: &TemporalMemoryResult) -> TimelineView {
    let traversal = &result.traversal;
    TimelineView {
        about: about.to_string(),
        direction: direction_name(traversal.direction()).to_string(),
        resolved_cursor: CoordinateView::from_coordinate(traversal.resolved_cursor()),
        included_dimensions: traversal.included_dimensions().to_vec(),
        missing_dimensions: traversal.missing_dimensions().to_vec(),
        entries: traversal
            .entries()
            .iter()
            .map(|entry| TimelineEntryView {
                ref_id: entry.ref_id().to_string(),
                kind: entry.kind().to_string(),
                text: entry.text().to_string(),
                coordinates: entry
                    .coordinates()
                    .iter()
                    .map(CoordinateView::from_coordinate)
                    .collect(),
            })
            .collect(),
        page: TimelinePageView {
            returned: traversal.page().returned(),
            total: traversal.page().total(),
            next_cursor: traversal.page().next_cursor().map(ToString::to_string),
        },
        missing: traversal.missing().to_vec(),
    }
}

fn direction_name(direction: TemporalDirection) -> &'static str {
    match direction {
        TemporalDirection::Goto => "goto",
        TemporalDirection::Near => "near",
        TemporalDirection::Rewind => "rewind",
        TemporalDirection::Forward => "forward",
    }
}

/// One proven path between two nodes, with the kernel's rendering of it.
#[derive(Debug, Clone, Serialize)]
pub struct TraceView {
    pub from: String,
    pub to: String,
    pub nodes: Vec<NodeView>,
    pub edges: Vec<EdgeView>,
    pub rendered: RenderedView,
    pub quality: QualityView,
}

pub fn trace_view(from: &str, to: &str, result: &GetContextPathResult) -> TraceView {
    let bundle = &result.path_bundle;
    let mut nodes = Vec::with_capacity(1 + bundle.neighbor_nodes().len());
    nodes.push(NodeView::from_bundle_node(bundle.root_node()));
    nodes.extend(
        bundle
            .neighbor_nodes()
            .iter()
            .map(NodeView::from_bundle_node),
    );
    TraceView {
        from: from.to_string(),
        to: to.to_string(),
        nodes,
        edges: bundle
            .relationships()
            .iter()
            .map(EdgeView::from_bundle_relationship)
            .collect(),
        rendered: RenderedView::from_rendered(&result.rendered),
        quality: QualityView::from_metrics(&result.rendered.quality),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AboutsView {
    pub abouts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InfoView {
    pub kernel_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<String>,
}
