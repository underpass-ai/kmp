mod ingest;
mod ref_boundary;
mod service;
mod types;
mod visual_projection;

pub use ingest::{ExistingMemoryRefs, crosses_abouts, translate_memory_ingest};
pub use ref_boundary::{
    validate_ref_token, validate_supplied_entry_ref, validate_supplied_evidence_ref,
    validate_supplied_member_ref,
};
pub use service::KernelMemoryApplicationService;
pub use types::{
    AskMemoryQuery, DEFAULT_RELATE_PAGE_ENTRIES, DEFAULT_TRACE_PAGE_ENTRIES, InspectMemoryQuery,
    InspectMemoryResult, InspectedEvidence, LabelPolicy, MAX_RELATE_PAGE_ENTRIES,
    MAX_TRACE_PAGE_ENTRIES, MemoryAcceptedCounts, MemoryAnswerPolicy, MemoryCoordinateData,
    MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData, MemoryIngestCommand,
    MemoryIngestOutcome, MemoryProvenanceData, MemoryRelationData, RelateMemoryQuery,
    RelatePageRequest, ResemblingLabelData, TemporalIncludeOptions, TemporalMemoryQuery,
    TemporalMemoryResult, TraceMemoryQuery, TracePageRequest, WakeMemoryQuery,
};
pub use visual_projection::{
    MAX_VISUAL_BINS, MAX_VISUAL_PAGE_ENTRIES, MAX_VISUAL_SOURCE_ENTRIES, TemporalAxisView,
    TemporalCoordinateView, VisualBin, VisualCluster, VisualEntry, VisualLevelOfDetail,
    VisualMetric, VisualProjectionPage, VisualProjectionQuery, VisualProjectionResult, VisualRange,
    VisualRelation, build_visual_projection,
};
