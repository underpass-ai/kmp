use kmp_domain::{
    DimensionSelection, ResolutionTier, TemporalAxis, TemporalCoordinate, TemporalCursor,
    TemporalDirection, TemporalSelection, TemporalWindow,
};

use crate::queries::{GetNodeDetailResult, GraphRelationshipView};

pub const DEFAULT_TRACE_PAGE_ENTRIES: usize = 64;
/// A relate page counts facts, declared relations, coordinate relations and
/// tensions alike; facts carry their text, so the page is kept smaller than
/// a trace page.
pub const DEFAULT_RELATE_PAGE_ENTRIES: usize = 32;
pub const MAX_RELATE_PAGE_ENTRIES: usize = 256;
pub const MAX_TRACE_PAGE_ENTRIES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryIngestCommand {
    pub about: String,
    pub memory: MemoryData,
    pub provenance: Option<MemoryProvenanceData>,
    pub idempotency_key: String,
    pub dry_run: bool,
    pub label_policy: LabelPolicy,
}

/// What an ingest does with a dimension that resembles a label the about
/// already holds: the same identifier up to case and separators, or the
/// same value under another key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelPolicy {
    /// Write it and say so: `warnings` and `resembling_labels` name the
    /// match, so vocabulary drift is seen when it happens.
    #[default]
    Warn,
    /// Refuse the ingest naming the match, unless the dimension carries the
    /// metadata that says the writer read the catalogue and insists.
    Refuse,
}

/// A label a write named beside the existing label it resembles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResemblingLabelData {
    pub key: String,
    pub value: String,
    pub existing_key: String,
    pub existing_value: String,
    pub kind: String,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MemoryData {
    pub dimensions: Vec<MemoryDimensionData>,
    pub entries: Vec<MemoryEntryData>,
    pub relations: Vec<MemoryRelationData>,
    pub evidence: Vec<MemoryEvidenceData>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MemoryDimensionData {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub metadata: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MemoryEntryData {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub coordinates: Vec<MemoryCoordinateData>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub metadata: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MemoryCoordinateData {
    pub dimension: String,
    pub scope_id: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub metadata: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MemoryRelationData {
    #[serde(rename = "from")]
    pub source_ref: String,
    #[serde(rename = "to")]
    pub target_ref: String,
    pub rel: String,
    #[serde(rename = "class")]
    pub semantic_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motivation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caused_by_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinate: Option<MemoryCoordinateData>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MemoryEvidenceData {
    pub id: String,
    pub supports: Vec<String>,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub metadata: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MemoryProvenanceData {
    pub source_kind: String,
    pub source_agent: String,
    pub observed_at: String,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryAcceptedCounts {
    pub entries: usize,
    pub relations: usize,
    pub evidence: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryIngestOutcome {
    pub about: String,
    pub memory_id: String,
    pub accepted: MemoryAcceptedCounts,
    pub read_after_write_ready: bool,
    pub warnings: Vec<String>,
    /// The dimension nodes this ingest declares for the first time, as
    /// namespaced ids: the labels the write created rather than reused.
    pub created_dimensions: Vec<String>,
    /// Labels this ingest declared that resemble one the about already
    /// holds, written under `LabelPolicy::Warn`.
    pub resembling_labels: Vec<ResemblingLabelData>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeMemoryQuery {
    pub about: String,
    pub role: String,
    pub intent: String,
    pub dimensions: DimensionSelection,
    pub token_budget: u32,
    pub depth: u32,
    pub max_tier: Option<ResolutionTier>,
    /// Cap on surfaced proof.evidence entries (None = unbounded). When set and
    /// the about has more, Wake returns the first `max_entries` and reports the
    /// withheld count via proof.frontier_size so the client near-expands.
    pub max_entries: Option<usize>,
    /// Which instants the packet stands on: the memory's frontier, one
    /// instant, or a half-open span on one clock.
    pub temporal: TemporalSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskMemoryQuery {
    pub about: String,
    pub question: String,
    /// The user's own words when `question` is the agent's rendering of them
    /// in the kernel's search language. Searched never; echoed and read
    /// against the question so a rendering that lost something says so.
    pub asked_as: Option<String>,
    pub answer_policy: MemoryAnswerPolicy,
    pub dimensions: DimensionSelection,
    pub token_budget: u32,
    pub depth: u32,
    pub max_tier: Option<ResolutionTier>,
    /// Cap on answer evidence entries after relevance filtering.
    pub max_entries: Option<usize>,
    /// Which instants the answer stands on: the memory's frontier, one
    /// instant, or a half-open span on one clock. Only what the selection
    /// admits competes, and the lifecycles are read as they stood then.
    pub temporal: TemporalSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalMemoryQuery {
    pub about: String,
    pub direction: TemporalDirection,
    pub axis: TemporalAxis,
    pub cursor: TemporalCursor,
    pub dimensions: DimensionSelection,
    pub window: TemporalWindow,
    pub limit_entries: Option<usize>,
    pub include: TemporalIncludeOptions,
    pub token_budget: u32,
    pub depth: u32,
    pub max_tier: Option<ResolutionTier>,
}

/// What memories of several abouts have to do with each other: the abouts
/// the selection names or resolves, read within one span on one clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelateMemoryQuery {
    pub about: String,
    pub dimensions: DimensionSelection,
    /// The span and clock the facts fall within; the memory's frontier when
    /// the caller named none.
    pub temporal: TemporalSelection,
    pub token_budget: u32,
    pub depth: u32,
    pub max_tier: Option<ResolutionTier>,
    pub page: RelatePageRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RelatePageRequest {
    pub entries: Option<usize>,
    pub cursor: Option<usize>,
}

impl RelatePageRequest {
    pub fn offset(&self) -> usize {
        self.cursor.unwrap_or_default()
    }

    pub fn entries_or_default(&self) -> usize {
        self.entries.unwrap_or(DEFAULT_RELATE_PAGE_ENTRIES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceMemoryQuery {
    pub about: String,
    pub from: String,
    pub to: String,
    pub role: String,
    pub token_budget: u32,
    pub page: TracePageRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TracePageRequest {
    pub entries: Option<usize>,
    pub cursor: Option<usize>,
}

impl TracePageRequest {
    pub fn offset(&self) -> usize {
        self.cursor.unwrap_or_default()
    }

    pub fn entries_or_default(&self) -> usize {
        self.entries.unwrap_or(DEFAULT_TRACE_PAGE_ENTRIES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectMemoryQuery {
    pub about: String,
    pub ref_id: String,
    pub include_details: bool,
    pub include_incoming: bool,
    pub include_outgoing: bool,
    pub include_raw: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct InspectMemoryResult {
    pub detail: GetNodeDetailResult,
    pub incoming: Vec<GraphRelationshipView>,
    pub outgoing: Vec<GraphRelationshipView>,
    pub evidence: Vec<InspectedEvidence>,
    pub raw_coordinates: Vec<TemporalCoordinate>,
    pub include_details: bool,
    pub include_raw: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct InspectedEvidence {
    pub detail: GetNodeDetailResult,
    pub supports: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MemoryAnswerPolicy {
    #[default]
    EvidenceOrUnknown,
    ShowConflicts,
    BestEffort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TemporalIncludeOptions {
    pub evidence: bool,
    pub relations: bool,
    pub raw_refs: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemporalMemoryResult {
    pub traversal: kmp_domain::TemporalTraversalResult,
    pub source_bundle: kmp_domain::KmpBundle,
    pub include: TemporalIncludeOptions,
    pub quality: kmp_domain::BundleQualityMetrics,
}
