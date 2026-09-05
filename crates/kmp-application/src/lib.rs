pub mod application_error;
pub mod commands;
pub mod kmp_application;
pub mod memory;
mod observability;
pub mod projection;
pub mod queries;

pub use application_error::ApplicationError;
pub use commands::{
    AcceptedVersion, CommandApplicationService, NoopProjectionWriter, UpdateContextChange,
    UpdateContextCommand, UpdateContextOutcome, UpdateContextUseCase,
    projection_mutations_for_context_event,
};
pub use kmp_application::KmpApplication;
pub use memory::{
    AskMemoryQuery, DEFAULT_RELATE_PAGE_ENTRIES, DEFAULT_TRACE_PAGE_ENTRIES, EntryLabelData,
    ExistingMemoryRefs, InspectMemoryQuery, InspectMemoryResult, InspectedEvidence,
    KernelMemoryApplicationService, LabelPolicy, MAX_RELATE_PAGE_ENTRIES, MAX_TRACE_PAGE_ENTRIES,
    MAX_VISUAL_BINS, MAX_VISUAL_PAGE_ENTRIES, MAX_VISUAL_SOURCE_ENTRIES, MemoryAcceptedCounts,
    MemoryAnswerPolicy, MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData,
    MemoryEvidenceData, MemoryIngestCommand, MemoryIngestOutcome, MemoryProvenanceData,
    MemoryRelabelCommand, MemoryRelabelOutcome, MemoryRelationData, RELABEL_ENTITY_KIND,
    RELABEL_METHOD, RelateMemoryQuery, RelatePageRequest, ResemblingLabelData, TemporalAxisView,
    TemporalCoordinateView, TemporalIncludeOptions, TemporalMemoryQuery, TemporalMemoryResult,
    TraceMemoryQuery, TracePageRequest, VisualBin, VisualCluster, VisualEntry, VisualLevelOfDetail,
    VisualMetric, VisualProjectionPage, VisualProjectionQuery, VisualProjectionResult, VisualRange,
    VisualRelation, WakeMemoryQuery, build_visual_projection, translate_memory_ingest,
    translate_memory_relabel, validate_ref_token, validate_supplied_entry_ref,
    validate_supplied_evidence_ref, validate_supplied_member_ref,
};
pub use observability::{
    ObservabilityExemplar, ObservabilityMetricPoint, ObservabilityProjection, ObservabilityQuery,
    ObservabilityQueryPort, ObservabilitySeries,
};
pub use projection::{
    GraphNodeMaterializedData, GraphNodeMaterializedEvent, GraphRelationMaterializedData,
    GraphRelationMaterializedEvent, NodeDetailMaterializedData, NodeDetailMaterializedEvent,
    ProjectionApplicationService, ProjectionEnvelope, ProjectionEvent, ProjectionEventHandler,
    ProjectionHandlingRequest, ProjectionHandlingResult, RelatedNodeExplanationData,
    RelatedNodeReference, RoutingProjectionWriter,
};
pub use queries::{
    BundleAssembler, ContextRenderOptions, DEFAULT_NATIVE_GRAPH_TRAVERSAL_DEPTH, EndpointHint,
    GetContextPathQuery, GetContextPathResult, GetContextPathUseCase, GetContextQuery,
    GetContextResult, GetContextUseCase, GetGraphRelationshipsQuery, GetGraphRelationshipsResult,
    GetGraphRelationshipsUseCase, GetNodeDetailQuery, GetNodeDetailResult, GetNodeDetailUseCase,
    GetNodeRelationshipsQuery, GetNodeRelationshipsResult, GetNodeRelationshipsUseCase,
    GraphNodeView, GraphRelationshipView, MAX_NATIVE_GRAPH_TRAVERSAL_DEPTH,
    MIN_NATIVE_GRAPH_TRAVERSAL_DEPTH, NodeCentricProjectionReader, NodeDetailView,
    QueryApplicationService, QueryTimingBreakdown, RehydrateSessionQuery, RehydrateSessionResult,
    RehydrateSessionUseCase, RenderedContext, RenderedTier, ScopeValidation, ValidateScopeQuery,
    ValidateScopeUseCase, clamp_native_graph_traversal_depth, render_graph_bundle_with_options,
};
