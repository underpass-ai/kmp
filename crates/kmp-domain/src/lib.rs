pub mod error;
pub mod language;
pub mod model;
pub mod plugins;
pub mod projection;
pub mod repositories;
pub mod value_objects;

pub use error::DomainError;
pub use model::{
    BundleNode, BundleNodeDetail, BundleRelationship, CoordinateRelation, CoordinateRelationKind,
    DECLARED_FROM_RELATE_METHOD, DeclaredEdge, FactState, INTENDED_NEW_LABEL_METADATA_KEY,
    KmpBundle, KmpStats, LabelResemblance, MAX_COORDINATE_RELATIONS, MAX_PROPOSALS_PER_FACT,
    ProofDependencyGroup, ProposalSignal, ProposedLink, RelatedFact, Relations, ResemblanceKind,
    TemporalEntry, TemporalEntrySelection, TemporalMemoryTraversal, TemporalTraversalRequest,
    TemporalTraversalResult, Tension, cap_proposals_per_fact, compare_temporal_coordinates,
    compare_temporal_instants, directed_relationship_path, label_resemblances, labels_by_entry,
    normalized_label_token, relate, rfc3339_from_epoch_seconds, temporal_instant_nanos,
    temporal_instant_rfc3339,
};
pub use model::{TemporalReadWindow, temporal_clock_instant};
pub use model::{
    TraceDimensionPolicy, TraceMaterialResult, TraceMaterialSelection, TraceProofRequirement,
    TraceRoutingStats, select_trace_material,
};
pub use projection::{
    ContextPathNeighborhood, GraphNodeMaterializedData, GraphNodeMaterializedEvent,
    GraphRelationMaterializedData, GraphRelationMaterializedEvent, NodeDetailMaterializedData,
    NodeDetailMaterializedEvent, NodeDetailProjection, NodeNeighborhood, NodeProjection,
    NodeRelationProjection, ProjectionCheckpoint, ProjectionEnvelope, ProjectionEvent,
    ProjectionEventHandler, ProjectionHandlingRequest, ProjectionHandlingResult,
    ProjectionMutation, RelatedNodeExplanationData, RelatedNodeReference,
};
pub use repositories::{
    AdjacencyPage, AdjacencyRequest, BoundedRelationReader, RelationDirection, RelationPosition,
};
pub use repositories::{COMMAND_RECEIPT_ENTITY_KIND, StoredCommandReceipt};
pub use repositories::{
    ContextEventChange, ContextEventStore, ContextRevision, ContextUpdatedEvent,
    GraphNeighborhoodReader, IdempotentOutcome, MemoryAboutIndexReader, NeighborhoodRequest,
    NodeDetailReader, NodeRelationshipReader, NodeRelationships, PortError, ProcessedEventStore,
    ProjectionCheckpointStore, ProjectionWriter, QualityMetricsObserver, QualityObservationContext,
    ReadSnapshotFuture, ReadSnapshotProvider, SnapshotSaveOptions, SnapshotStore, TokenEstimator,
};
pub use value_objects::MemoryReceiptRef;
pub use value_objects::{
    BundleMetadata, BundleQualityMetrics, CaseId, CoordinateOrigin, DimensionScopeMode,
    DimensionSelection, DimensionSelectionMode, EntryLabels, LabelSelector, LabelSelectorOperator,
    MemoryDimensionIdentity, Provenance, Role, SourceKind, TemporalAxis, TemporalCoordinate,
    TemporalCursor, TemporalDirection, TemporalInterval, TemporalSelection, TemporalWindow,
    bare_label_value,
};
pub use value_objects::{KmpMode, ResolutionTier, TierBudget};
pub use value_objects::{
    KnownMemoryRelationType, MemoryRelationQuality, MemoryRelationSpec, MemoryRelationType,
};
pub use value_objects::{MAX_RELATION_SIGNAL_WEIGHT, RelationSignal};
pub use value_objects::{
    QuestionRendering, QuestionRenderingFault, SearchSummary, SearchSummaryFault,
};
pub use value_objects::{RelationExplanation, RelationSemanticClass};

pub use model::{
    EvidenceMissingWitness, EvidencePathBinding, EvidencePathBindings, EvidencePathCandidate,
    EvidencePathGroup, EvidencePathRequest, EvidencePathResult, EvidencePathRole,
    EvidencePathStatus, search_evidence_paths,
};
pub use model::{
    TraceProofObject, TraceProofResult, TraceRelationStep, TraceRoute, TraceSearchLimits,
    TraceSearchRequest, TraceSearchResult, TraceSearchStop, bounded_trace_search,
};
pub use repositories::TraceSnapshotReader;
pub use repositories::{NodeCardStore, NodeCardWriteFuture};
pub use model::{AuthorNodeCard, NodeCardExpectation, NodeCardRejection, node_card_policy};
pub use model::{
    MAX_EXPANSION_REFS, TraceBodyAdmission, TraceBodyOptions, TraceManifestDigest,
    trace_body_admission,
};
pub use projection::NodeBodyDescriptor;
pub use value_objects::{
    NodeCard, NodeCardPresentation, NodeCardStamp, NodeCardStatus, TraceBodyDelivery,
    TraceBodyState, TraceCompactSummary, TraceExpansionPlan, TraceExpansionRefusal,
};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        BundleMetadata, BundleNode, CaseId, DomainError, KmpBundle, RelationExplanation,
        RelationSemanticClass, Role,
    };

    #[test]
    fn case_id_requires_a_value() {
        let error = CaseId::new("   ").expect_err("empty case id must fail");
        assert_eq!(error, DomainError::EmptyValue("case_id"));
    }

    #[test]
    fn bundle_tracks_graph_native_state() {
        let case_id = CaseId::new("case-123").expect("case id is valid");
        let role = Role::new("developer").expect("role is valid");
        let root = BundleNode::new(
            "case-123",
            "case",
            "Case 123",
            "Projection snapshot loaded",
            "ACTIVE",
            vec!["ProjectionNode".to_string()],
            BTreeMap::new(),
        );
        let bundle = KmpBundle::new(
            case_id,
            role.clone(),
            root,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            BundleMetadata::initial("0.1.0"),
        )
        .expect("bundle should be valid");

        assert_eq!(bundle.role().as_str(), "developer");
        assert_eq!(bundle.root_node().summary(), "Projection snapshot loaded");
        assert_eq!(bundle.stats().selected_nodes(), 1);
    }

    #[test]
    fn bundle_rejects_relationships_outside_the_bundle() {
        let bundle = KmpBundle::new(
            CaseId::new("case-123").expect("case id is valid"),
            Role::new("developer").expect("role is valid"),
            BundleNode::new(
                "case-123",
                "case",
                "Case 123",
                "Projection snapshot loaded",
                "ACTIVE",
                vec![],
                BTreeMap::new(),
            ),
            Vec::new(),
            vec![super::BundleRelationship::new(
                "case-123",
                "node-missing",
                "RELATES_TO",
                RelationExplanation::new(RelationSemanticClass::Structural),
            )],
            Vec::new(),
            BundleMetadata::initial("0.1.0"),
        )
        .expect_err("invalid relationship should fail");

        assert_eq!(
            bundle,
            DomainError::InvalidState(
                "relationship `case-123` -> `node-missing` references nodes outside the bundle"
                    .to_string()
            )
        );
    }
}
