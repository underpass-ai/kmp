//! The viewer's wire format: plain serde views over application results.
//!
//! Projections, never the aggregates — the kernel's domain types gain fields
//! as the domain needs them, and a browser client that read them directly
//! would inherit each one as a contract. Everything here is what the UI
//! renders, nothing more.

use std::collections::BTreeMap;

use kmp_application::{
    GetContextPathResult, GetContextResult, GetNodeDetailResult, GraphNodeView,
    GraphRelationshipView, InspectMemoryResult, RenderedContext, TemporalMemoryResult,
};
use kmp_domain::{
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
            occurred_at: explanation.occurred_at().map(readable_time),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderedView {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub token_count: u32,
}

impl RenderedView {
    fn from_rendered(rendered: &RenderedContext) -> Self {
        Self {
            content: rendered.content.clone(),
            content_hash: real_hash(&rendered.content_hash),
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
            occurred_at: coordinate.occurred_at().map(readable_time),
            observed_at: coordinate.observed_at().map(readable_time),
            ingested_at: coordinate.ingested_at().map(readable_time),
            valid_from: coordinate.valid_from().map(readable_time),
            valid_until: coordinate.valid_until().map(readable_time),
        }
    }
}

/// A time a reader can read.
///
/// Times are stored in a zero-padded, lexicographically sortable form —
/// `unix:<seconds + offset>:<nanos>` — because byte order has to equal time
/// order in the store. That is a storage concern, and it was reaching the
/// timeline's time column, where `unix:101786903200:000000000` sat exactly
/// where a date belongs. Anything that is not that form is passed through
/// untouched: times ingested as RFC3339 are already readable.
pub fn readable_time(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    match parse_sortable_seconds(value) {
        Some(seconds) => rfc3339_utc(seconds),
        None => value.to_string(),
    }
}

/// A snapshot hash, or nothing.
///
/// `BundleMetadata::initial` stamps the literal `"pending"` and the embedded
/// edition never replaces it, so the status bar read `SNAPSHOT pending`
/// forever — a word that looks like a state on its way somewhere. A field
/// that can only ever say one thing tells the reader nothing; absent is the
/// honest answer until a hash is actually computed.
fn real_hash(value: &str) -> Option<String> {
    match value {
        "" | "pending" => None,
        hash => Some(hash.to_string()),
    }
}

/// The offset the sortable form adds so pre-epoch times stay non-negative.
const UNIX_SORT_OFFSET: i64 = 100_000_000_000;

fn parse_sortable_seconds(value: &str) -> Option<i64> {
    let (seconds, nanos) = value.strip_prefix("unix:")?.split_once(':')?;
    if !nanos.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(seconds.parse::<i64>().ok()? - UNIX_SORT_OFFSET)
}

/// Epoch seconds to `YYYY-MM-DDTHH:MM:SSZ`, without a date library.
///
/// Howard Hinnant's civil-from-days, which is exact for every day this store
/// can hold. The crate ships no dependency the embedded edition does not
/// already carry, and a viewer is not the place to start.
fn rfc3339_utc(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let time_of_day = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (
        time_of_day / 3_600,
        (time_of_day % 3_600) / 60,
        time_of_day % 60,
    );

    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// One recalled neighborhood, ready to draw: nodes, typed edges, details, and
/// the kernel's rendered context with its quality account.
#[derive(Debug, Clone, Serialize)]
pub struct GraphView {
    pub about: String,
    pub root_id: String,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
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
        content_hash: real_hash(&bundle.metadata().content_hash),
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
                content_hash: real_hash(detail.content_hash()),
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
        content_hash: real_hash(&detail.content_hash),
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

#[cfg(test)]
mod tests {
    use super::{readable_time, rfc3339_utc};

    #[test]
    fn a_sortable_time_becomes_a_date_a_reader_can_read() {
        // The exact string this store put in the timeline's time column.
        assert_eq!(
            readable_time("unix:101786903200:000000000"),
            "2026-08-16T18:00:00Z"
        );
    }

    #[test]
    fn anything_that_is_not_the_sortable_form_passes_through_untouched() {
        for value in [
            "2026-07-01T10:00:00Z",
            "unix:not-a-number:000000000",
            "unix:101786903200",
            "",
        ] {
            assert_eq!(readable_time(value), value, "mangled {value}");
        }
    }

    #[test]
    fn the_calendar_holds_at_the_edges() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(-1), "1969-12-31T23:59:59Z");
        // A leap day, and the century rule that trips naive implementations.
        assert_eq!(rfc3339_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_utc(4_107_542_400), "2100-03-01T00:00:00Z");
    }
}
