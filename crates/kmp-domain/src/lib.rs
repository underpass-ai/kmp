pub mod error;
pub mod model;
pub mod plugins;
pub mod projection;
pub mod repositories;
pub mod value_objects;

pub use error::DomainError;
pub use model::{
    BundleNode, BundleNodeDetail, BundleRelationship, KmpBundle, KmpStats, TemporalEntry,
    TemporalMemoryTraversal, TemporalTraversalRequest, TemporalTraversalResult,
    compare_temporal_instants, directed_relationship_path,
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
    ContextEventChange, ContextEventStore, ContextUpdatedEvent, GraphNeighborhoodReader,
    IdempotentOutcome, MemoryAboutIndexReader, NodeDetailReader, NodeRelationshipReader,
    NodeRelationships, PortError, ProcessedEventStore, ProjectionCheckpointStore, ProjectionWriter,
    QualityMetricsObserver, QualityObservationContext, SnapshotSaveOptions, SnapshotStore,
    TokenEstimator,
};
pub use value_objects::{
    BundleMetadata, BundleQualityMetrics, CaseId, DimensionScopeMode, DimensionSelection,
    DimensionSelectionMode, MemoryDimensionIdentity, Provenance, Role, SourceKind, TemporalAxis,
    TemporalCoordinate, TemporalCursor, TemporalDirection, TemporalWindow,
};
pub use value_objects::{KmpMode, ResolutionTier, TierBudget};
pub use value_objects::{
    KnownMemoryRelationType, MemoryRelationQuality, MemoryRelationSpec, MemoryRelationType,
};
pub use value_objects::{RelationExplanation, RelationSemanticClass};

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
