use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use prost_types::Timestamp;
use rehydration_application::{
    AcceptedVersion, ApplicationError, CommandApplicationService, GetContextQuery,
    KernelMemoryApplicationService, MAX_TRACE_PAGE_ENTRIES, NoopProjectionWriter,
    QueryApplicationService, RehydrateSessionResult, UpdateContextUseCase,
};
use rehydration_domain::{
    BundleMetadata, BundleNode, BundleNodeDetail, BundleRelationship, CaseId, ContextEventStore,
    ContextPathNeighborhood, GraphNeighborhoodReader, MemoryAboutIndexReader, NodeDetailProjection,
    NodeDetailReader, NodeNeighborhood, NodeProjection, NodeRelationProjection,
    NodeRelationshipReader, NodeRelationships, PortError, ProjectionMutation, ProjectionWriter,
    RehydrationBundle, RelationExplanation, RelationSemanticClass, Role, SnapshotSaveOptions,
    SnapshotStore,
};
use rehydration_observability::quality_observers::NoopQualityObserver;
use rehydration_proto::v1beta1::{
    AnswerPolicy, AskRequest, ContextChange, ContextChangeOperation,
    DimensionScopeMode as ProtoDimensionScopeMode, DimensionSelection as ProtoDimensionSelection,
    DimensionSelectionMode as ProtoDimensionSelectionMode, ForwardRequest, GetContextPathRequest,
    GetContextRequest, GetNodeDetailRequest, GotoRequest, IngestRequest, InspectInclude,
    InspectRequest, Memory, MemoryBudget, MemoryConfidence, MemoryDetailLevel, MemoryDimension,
    MemoryEntry, MemoryEvidence, MemoryProvenance, MemoryRelation, MemorySemanticClass,
    MemorySourceKind, NearRequest, PageRequest, ResolutionTier, RewindRequest,
    TemporalCoordinate as ProtoTemporalCoordinate, TemporalCursor as ProtoTemporalCursor,
    TemporalDirection as ProtoTemporalDirection, TemporalInclude, TemporalLimit,
    TemporalMoveRequest, TemporalWindow as ProtoTemporalWindow, TraceRequest, UpdateContextRequest,
    ValidateScopeRequest, WakeRequest, context_command_service_server::ContextCommandService,
    context_query_service_server::ContextQueryService,
    kernel_memory_service_server::KernelMemoryService,
};
use rehydration_testkit::InMemoryContextEventStore;
use tokio::sync::Mutex;
use tonic::Request;

use super::command_grpc_service_v1beta1::CommandGrpcServiceV1Beta1;
use super::grpc_server::GrpcServer;
use super::memory_grpc_service_v1beta1::MemoryGrpcServiceV1Beta1;
use super::proto_mapping_v1beta1::{
    proto_accepted_version_v1beta1, proto_bundle_from_single_role_v1beta1,
    proto_bundle_node_detail_v1beta1, proto_bundle_node_v1beta1, proto_bundle_relationship_v1beta1,
    proto_rehydrate_session_response_v1beta1,
};
use super::query_grpc_service_v1beta1::QueryGrpcServiceV1Beta1;
use super::support::{map_application_error, proto_duration, trim_to_option};

struct EmptyGraphNeighborhoodReader;

impl GraphNeighborhoodReader for EmptyGraphNeighborhoodReader {
    async fn load_neighborhood(
        &self,
        _root_node_id: &str,
        _depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        Ok(None)
    }

    async fn load_context_path(
        &self,
        _root_node_id: &str,
        _target_node_id: &str,
        _subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        Ok(None)
    }
}

impl MemoryAboutIndexReader for EmptyGraphNeighborhoodReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        Ok(Vec::new())
    }
}

impl NodeRelationshipReader for EmptyGraphNeighborhoodReader {
    async fn load_node_relationships(
        &self,
        _node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        Ok(None)
    }
}

impl ProjectionWriter for EmptyGraphNeighborhoodReader {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

struct RecordingGraphNeighborhoodReader {
    depths: Arc<Mutex<Vec<u32>>>,
}

impl GraphNeighborhoodReader for RecordingGraphNeighborhoodReader {
    async fn load_neighborhood(
        &self,
        _root_node_id: &str,
        depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        self.depths.lock().await.push(depth);
        Ok(None)
    }

    async fn load_context_path(
        &self,
        _root_node_id: &str,
        _target_node_id: &str,
        subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        self.depths.lock().await.push(subtree_depth);
        Ok(None)
    }
}

impl MemoryAboutIndexReader for RecordingGraphNeighborhoodReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        Ok(Vec::new())
    }
}

impl NodeRelationshipReader for RecordingGraphNeighborhoodReader {
    async fn load_node_relationships(
        &self,
        _node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        Ok(None)
    }
}

struct EmptyNodeDetailReader;

impl NodeDetailReader for EmptyNodeDetailReader {
    async fn load_node_detail(
        &self,
        _node_id: &str,
    ) -> Result<Option<NodeDetailProjection>, PortError> {
        Ok(None)
    }

    async fn load_node_details_batch(
        &self,
        node_ids: Vec<String>,
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        let mut results = Vec::with_capacity(node_ids.len());
        for node_id in &node_ids {
            results.push(self.load_node_detail(node_id).await?);
        }
        Ok(results)
    }
}

impl ProjectionWriter for EmptyNodeDetailReader {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

struct SeededGraphNeighborhoodReader;

impl GraphNeighborhoodReader for SeededGraphNeighborhoodReader {
    async fn load_neighborhood(
        &self,
        root_node_id: &str,
        _depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        match root_node_id {
            "node-123" => Ok(Some(sample_node_neighborhood("node-123", "ACTIVE"))),
            "graph-only" => Ok(Some(sample_node_neighborhood("graph-only", "READY"))),
            "question:evidence-answer" => Ok(Some(NodeNeighborhood {
                root: temporal_projection(
                    "question:evidence-answer",
                    "memory_anchor",
                    "Evidence-backed answer memory",
                ),
                neighbors: vec![
                    temporal_projection(
                        "about:question:evidence-answer:dimension:conversation",
                        "memory_dimension",
                        "Evidence answer conversation",
                    ),
                    temporal_projection("claim:answer", "claim", "Answer claim"),
                    temporal_projection("evidence:answer", "memory_evidence", "Answer evidence"),
                ],
                relations: vec![
                    temporal_contains_entry(
                        "about:question:evidence-answer:dimension:conversation",
                        "claim:answer",
                        "conversation",
                        1,
                        Some(sort_time(120)),
                        None,
                        None,
                    ),
                    NodeRelationProjection {
                        source_node_id: "evidence:answer".to_string(),
                        target_node_id: "claim:answer".to_string(),
                        relation_type: "supports".to_string(),
                        explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                            .with_rationale("Explicit evidence supports the answer claim.")
                            .with_confidence("high"),
                    },
                ],
            })),
            "question:conflict-answer" => Ok(Some(NodeNeighborhood {
                root: temporal_projection(
                    "question:conflict-answer",
                    "memory_anchor",
                    "Conflict-backed answer memory",
                ),
                neighbors: vec![
                    temporal_projection(
                        "about:question:conflict-answer:dimension:conversation",
                        "memory_dimension",
                        "Conflict answer conversation",
                    ),
                    temporal_projection("claim:cache-enabled", "claim", "Cache was enabled."),
                    temporal_projection(
                        "evidence:cache-enabled",
                        "memory_evidence",
                        "Cache enabled evidence",
                    ),
                    temporal_projection("claim:cache-disabled", "claim", "Cache was disabled."),
                    temporal_projection(
                        "evidence:cache-disabled",
                        "memory_evidence",
                        "Cache disabled evidence",
                    ),
                ],
                relations: vec![
                    temporal_contains_entry(
                        "about:question:conflict-answer:dimension:conversation",
                        "claim:cache-enabled",
                        "conversation",
                        1,
                        Some(sort_time(130)),
                        None,
                        None,
                    ),
                    temporal_contains_entry(
                        "about:question:conflict-answer:dimension:conversation",
                        "claim:cache-disabled",
                        "conversation",
                        2,
                        Some(sort_time(131)),
                        None,
                        None,
                    ),
                    NodeRelationProjection {
                        source_node_id: "evidence:cache-enabled".to_string(),
                        target_node_id: "claim:cache-enabled".to_string(),
                        relation_type: "supports".to_string(),
                        explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                            .with_rationale("Explicit evidence supports the enabled claim.")
                            .with_confidence("high"),
                    },
                    NodeRelationProjection {
                        source_node_id: "evidence:cache-disabled".to_string(),
                        target_node_id: "claim:cache-disabled".to_string(),
                        relation_type: "supports".to_string(),
                        explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                            .with_rationale("Explicit evidence supports the disabled claim.")
                            .with_confidence("high"),
                    },
                    NodeRelationProjection {
                        source_node_id: "claim:cache-enabled".to_string(),
                        target_node_id: "claim:cache-disabled".to_string(),
                        relation_type: "contradicts".to_string(),
                        explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                            .with_rationale("The cache cannot be both enabled and disabled in the same observed state.")
                            .with_confidence("high"),
                    },
                ],
            })),
            _ => Ok(None),
        }
    }

    async fn load_context_path(
        &self,
        root_node_id: &str,
        target_node_id: &str,
        _subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        Ok(
            (root_node_id == "node-123" && target_node_id == "node-789").then_some(
                ContextPathNeighborhood {
                    root: sample_node_neighborhood("node-123", "ACTIVE").root,
                    neighbors: vec![
                        NodeProjection {
                            node_id: "node-456".to_string(),
                            node_kind: "task".to_string(),
                            title: "Node node-456".to_string(),
                            summary: "Summary for node-456".to_string(),
                            status: "OPEN".to_string(),
                            labels: vec!["Task".to_string()],
                            properties: [("owner".to_string(), "ops".to_string())]
                                .into_iter()
                                .collect(),
                            provenance: None,
                        },
                        NodeProjection {
                            node_id: "node-789".to_string(),
                            node_kind: "task".to_string(),
                            title: "Node node-789".to_string(),
                            summary: "Summary for node-789".to_string(),
                            status: "READY".to_string(),
                            labels: vec!["Task".to_string()],
                            properties: [("owner".to_string(), "ops".to_string())]
                                .into_iter()
                                .collect(),
                            provenance: None,
                        },
                    ],
                    relations: vec![
                        NodeRelationProjection {
                            source_node_id: "node-123".to_string(),
                            target_node_id: "node-456".to_string(),
                            relation_type: "TRIGGERS".to_string(),
                            explanation: RelationExplanation::new(RelationSemanticClass::Causal)
                                .with_rationale("recovery triggered")
                                .with_sequence(1),
                        },
                        NodeRelationProjection {
                            source_node_id: "node-456".to_string(),
                            target_node_id: "node-789".to_string(),
                            relation_type: "PRODUCES".to_string(),
                            explanation: RelationExplanation::new(RelationSemanticClass::Causal)
                                .with_rationale("task produces artifact")
                                .with_sequence(2),
                        },
                    ],
                    path_node_ids: vec![
                        "node-123".to_string(),
                        "node-456".to_string(),
                        "node-789".to_string(),
                    ],
                },
            ),
        )
    }
}

impl MemoryAboutIndexReader for SeededGraphNeighborhoodReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        Ok(vec![
            "node-123".to_string(),
            "graph-only".to_string(),
            "question:evidence-answer".to_string(),
            "question:conflict-answer".to_string(),
        ])
    }
}

impl NodeRelationshipReader for SeededGraphNeighborhoodReader {
    async fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        Ok(match node_id {
            "node-123" => Some(NodeRelationships {
                incoming: vec![
                    temporal_contains_entry(
                        "about:node-123:dimension:conversation",
                        "node-123",
                        "conversation",
                        4,
                        Some(sort_time(140)),
                        None,
                        None,
                    ),
                    NodeRelationProjection {
                        source_node_id: "node-000".to_string(),
                        target_node_id: "node-123".to_string(),
                        relation_type: "supports".to_string(),
                        explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                            .with_rationale("seeded predecessor supports the inspected node")
                            .with_confidence("high"),
                    },
                ],
                outgoing: vec![NodeRelationProjection {
                    source_node_id: "node-123".to_string(),
                    target_node_id: "node-456".to_string(),
                    relation_type: "TRIGGERS".to_string(),
                    explanation: RelationExplanation::new(RelationSemanticClass::Causal)
                        .with_rationale("recovery triggered")
                        .with_sequence(1),
                }],
            }),
            "question:evidence-answer" | "question:conflict-answer" => {
                Some(NodeRelationships::default())
            }
            "graph-only" => Some(NodeRelationships::default()),
            _ => None,
        })
    }
}

struct RecordingSeededGraphNeighborhoodReader {
    context_path_depths: Arc<Mutex<Vec<u32>>>,
}

impl GraphNeighborhoodReader for RecordingSeededGraphNeighborhoodReader {
    async fn load_neighborhood(
        &self,
        root_node_id: &str,
        depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        let seeded = SeededGraphNeighborhoodReader;
        seeded.load_neighborhood(root_node_id, depth).await
    }

    async fn load_context_path(
        &self,
        root_node_id: &str,
        target_node_id: &str,
        subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        self.context_path_depths.lock().await.push(subtree_depth);
        let seeded = SeededGraphNeighborhoodReader;
        seeded
            .load_context_path(root_node_id, target_node_id, subtree_depth)
            .await
    }
}

impl MemoryAboutIndexReader for RecordingSeededGraphNeighborhoodReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        let seeded = SeededGraphNeighborhoodReader;
        seeded.list_memory_abouts().await
    }
}

impl NodeRelationshipReader for RecordingSeededGraphNeighborhoodReader {
    async fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        let seeded = SeededGraphNeighborhoodReader;
        seeded.load_node_relationships(node_id).await
    }
}

struct SeededNodeDetailReader;

impl NodeDetailReader for SeededNodeDetailReader {
    async fn load_node_detail(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeDetailProjection>, PortError> {
        Ok(match node_id {
            "node-123" => Some(NodeDetailProjection {
                node_id: "node-123".to_string(),
                detail: "Expanded detail".to_string(),
                content_hash: "hash-1".to_string(),
                revision: 2,
            }),
            "question:evidence-answer" => Some(NodeDetailProjection {
                node_id: "question:evidence-answer".to_string(),
                detail: "Anchor summary must not become the Ask answer.".to_string(),
                content_hash: "hash-anchor-answer".to_string(),
                revision: 1,
            }),
            "claim:answer" => Some(NodeDetailProjection {
                node_id: "claim:answer".to_string(),
                detail: "Claim detail must not outrank explicit evidence.".to_string(),
                content_hash: "hash-claim-answer".to_string(),
                revision: 1,
            }),
            "evidence:answer" => Some(NodeDetailProjection {
                node_id: "evidence:answer".to_string(),
                detail: "Explicit memory evidence is the deterministic Ask answer.".to_string(),
                content_hash: "hash-evidence-answer".to_string(),
                revision: 1,
            }),
            "question:conflict-answer" => Some(NodeDetailProjection {
                node_id: "question:conflict-answer".to_string(),
                detail: "Anchor detail must not hide explicit conflicts.".to_string(),
                content_hash: "hash-anchor-conflict".to_string(),
                revision: 1,
            }),
            "claim:cache-enabled" => Some(NodeDetailProjection {
                node_id: "claim:cache-enabled".to_string(),
                detail: "Claim detail: cache was enabled.".to_string(),
                content_hash: "hash-claim-cache-enabled".to_string(),
                revision: 1,
            }),
            "claim:cache-disabled" => Some(NodeDetailProjection {
                node_id: "claim:cache-disabled".to_string(),
                detail: "Claim detail: cache was disabled.".to_string(),
                content_hash: "hash-claim-cache-disabled".to_string(),
                revision: 1,
            }),
            "evidence:cache-enabled" => Some(NodeDetailProjection {
                node_id: "evidence:cache-enabled".to_string(),
                detail: "Metric sample reports cache=true at the incident checkpoint.".to_string(),
                content_hash: "hash-evidence-cache-enabled".to_string(),
                revision: 1,
            }),
            "evidence:cache-disabled" => Some(NodeDetailProjection {
                node_id: "evidence:cache-disabled".to_string(),
                detail: "Config snapshot reports cache=false at the incident checkpoint."
                    .to_string(),
                content_hash: "hash-evidence-cache-disabled".to_string(),
                revision: 1,
            }),
            "node-456" => Some(NodeDetailProjection {
                node_id: "node-456".to_string(),
                detail: "Middle detail".to_string(),
                content_hash: "hash-456".to_string(),
                revision: 3,
            }),
            "node-789" => Some(NodeDetailProjection {
                node_id: "node-789".to_string(),
                detail: "Target detail".to_string(),
                content_hash: "hash-789".to_string(),
                revision: 4,
            }),
            "orphan-detail" => Some(NodeDetailProjection {
                node_id: "orphan-detail".to_string(),
                detail: "orphaned".to_string(),
                content_hash: "hash-orphan".to_string(),
                revision: 1,
            }),
            _ => None,
        })
    }

    async fn load_node_details_batch(
        &self,
        node_ids: Vec<String>,
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        let mut results = Vec::with_capacity(node_ids.len());
        for node_id in &node_ids {
            results.push(self.load_node_detail(node_id).await?);
        }
        Ok(results)
    }
}

struct TemporalGraphNeighborhoodReader;

impl GraphNeighborhoodReader for TemporalGraphNeighborhoodReader {
    async fn load_neighborhood(
        &self,
        root_node_id: &str,
        _depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        match root_node_id {
            "question:830ce83f" => Ok(Some(NodeNeighborhood {
                root: temporal_projection(
                    "question:830ce83f",
                    "memory_anchor",
                    "Rachel relocation memory",
                ),
                neighbors: vec![
                    temporal_projection(
                        "about:question:830ce83f:dimension:conversation",
                        "memory_dimension",
                        "Rachel relocation discussion",
                    ),
                    temporal_projection(
                        "about:question:830ce83f:dimension:entity",
                        "memory_dimension",
                        "Rachel",
                    ),
                    temporal_projection(
                        "about:question:830ce83f:dimension:benchmark_record",
                        "memory_dimension",
                        "Benchmark item",
                    ),
                    temporal_projection(
                        "claim:rachel-denver",
                        "claim",
                        "Rachel said she was moving to Denver.",
                    ),
                    temporal_projection(
                        "claim:rachel-austin",
                        "claim",
                        "Rachel later corrected the destination to Austin.",
                    ),
                ],
                relations: vec![
                    temporal_contains_entry(
                        "about:question:830ce83f:dimension:conversation",
                        "claim:rachel-denver",
                        "conversation",
                        1,
                        Some(sort_time(100)),
                        None,
                        None,
                    ),
                    temporal_contains_entry(
                        "about:question:830ce83f:dimension:entity",
                        "claim:rachel-denver",
                        "entity",
                        1,
                        None,
                        Some(sort_time(100)),
                        None,
                    ),
                    temporal_contains_entry(
                        "about:question:830ce83f:dimension:conversation",
                        "claim:rachel-austin",
                        "conversation",
                        2,
                        Some(sort_time(105)),
                        None,
                        None,
                    ),
                    temporal_contains_entry(
                        "about:question:830ce83f:dimension:entity",
                        "claim:rachel-austin",
                        "entity",
                        2,
                        None,
                        Some(sort_time(105)),
                        None,
                    ),
                    temporal_contains_entry(
                        "about:question:830ce83f:dimension:benchmark_record",
                        "claim:rachel-austin",
                        "benchmark_record",
                        7,
                        None,
                        None,
                        Some(7),
                    ),
                    NodeRelationProjection {
                        source_node_id: "claim:rachel-austin".to_string(),
                        target_node_id: "claim:rachel-denver".to_string(),
                        relation_type: "supersedes".to_string(),
                        explanation: RelationExplanation::new(RelationSemanticClass::Evidential)
                            .with_rationale("The later statement corrects the earlier destination.")
                            .with_confidence("high"),
                    },
                ],
            })),
            "question:porto" => Ok(Some(NodeNeighborhood {
                root: temporal_projection("question:porto", "memory_anchor", "Porto travel memory"),
                neighbors: vec![
                    temporal_projection(
                        "about:question:porto:dimension:conversation",
                        "memory_dimension",
                        "Porto travel discussion",
                    ),
                    temporal_projection(
                        "claim:porto-lisbon",
                        "claim",
                        "The Porto plan later included a Lisbon stop.",
                    ),
                ],
                relations: vec![temporal_contains_entry(
                    "about:question:porto:dimension:conversation",
                    "claim:porto-lisbon",
                    "conversation",
                    3,
                    Some(sort_time(110)),
                    None,
                    None,
                )],
            })),
            _ => Ok(None),
        }
    }

    async fn load_context_path(
        &self,
        _root_node_id: &str,
        _target_node_id: &str,
        _subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        Ok(None)
    }
}

impl MemoryAboutIndexReader for TemporalGraphNeighborhoodReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        Ok(vec![
            "question:830ce83f".to_string(),
            "question:porto".to_string(),
        ])
    }
}

impl NodeRelationshipReader for TemporalGraphNeighborhoodReader {
    async fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        Ok(match node_id {
            "question:830ce83f" | "question:porto" => Some(NodeRelationships::default()),
            _ => None,
        })
    }
}

struct NoopSnapshotStore;

impl SnapshotStore for NoopSnapshotStore {
    async fn save_bundle_with_options(
        &self,
        _bundle: &RehydrationBundle,
        _options: SnapshotSaveOptions,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

fn memory_service<G, D>(
    graph_reader: G,
    detail_reader: D,
) -> MemoryGrpcServiceV1Beta1<
    G,
    D,
    NoopSnapshotStore,
    InMemoryContextEventStore,
    NoopProjectionWriter,
>
where
    G: GraphNeighborhoodReader
        + MemoryAboutIndexReader
        + NodeRelationshipReader
        + Send
        + Sync
        + 'static,
    D: NodeDetailReader + Send + Sync + 'static,
{
    memory_service_with_store(
        graph_reader,
        detail_reader,
        Arc::new(InMemoryContextEventStore::new()),
    )
}

fn memory_service_with_store<G, D>(
    graph_reader: G,
    detail_reader: D,
    event_store: Arc<InMemoryContextEventStore>,
) -> MemoryGrpcServiceV1Beta1<
    G,
    D,
    NoopSnapshotStore,
    InMemoryContextEventStore,
    NoopProjectionWriter,
>
where
    G: GraphNeighborhoodReader
        + MemoryAboutIndexReader
        + NodeRelationshipReader
        + Send
        + Sync
        + 'static,
    D: NodeDetailReader + Send + Sync + 'static,
{
    let query_application = Arc::new(QueryApplicationService::new(
        Arc::new(graph_reader),
        Arc::new(detail_reader),
        Arc::new(NoopSnapshotStore),
        "0.1.0",
    ));
    let command_application = Arc::new(CommandApplicationService::new(Arc::new(
        UpdateContextUseCase::new(event_store, "0.1.0"),
    )));
    let application = Arc::new(KernelMemoryApplicationService::new(
        query_application,
        command_application,
    ));

    MemoryGrpcServiceV1Beta1::new(application)
}

#[test]
fn describe_mentions_bind_address() {
    let config = rehydration_config::AppConfig {
        service_name: "rehydration-kernel".to_string(),
        grpc_bind: "127.0.0.1:50054".to_string(),
        grpc_tls: rehydration_config::GrpcTlsConfig::disabled(),
        graph_uri: "neo4j://localhost:7687".to_string(),
        detail_uri: "redis://localhost:6379".to_string(),
        snapshot_uri: "redis://localhost:6379".to_string(),
        events_subject_prefix: "rehydration".to_string(),
    };
    let server = GrpcServer::new(
        config,
        EmptyGraphNeighborhoodReader,
        EmptyNodeDetailReader,
        NoopSnapshotStore,
        InMemoryContextEventStore::new(),
        Arc::new(NoopQualityObserver),
    );

    assert!(server.describe().contains("127.0.0.1:50054"));
    assert_eq!(server.bootstrap_request().root_node_id, "bootstrap-node");
    let descriptor_set = std::hint::black_box(server.descriptor_set());
    assert!(!descriptor_set.is_empty());
    assert!(
        descriptor_set
            .windows(b"KernelMemoryService".len())
            .any(|window| window == b"KernelMemoryService")
    );
}

#[tokio::test]
async fn grpc_server_application_accessors_return_callable_services() {
    let config = rehydration_config::AppConfig {
        service_name: "rehydration-kernel".to_string(),
        grpc_bind: "127.0.0.1:50054".to_string(),
        grpc_tls: rehydration_config::GrpcTlsConfig::disabled(),
        graph_uri: "neo4j://localhost:7687".to_string(),
        detail_uri: "redis://localhost:6379".to_string(),
        snapshot_uri: "redis://localhost:6379".to_string(),
        events_subject_prefix: "rehydration".to_string(),
    };
    let server = GrpcServer::new(
        config,
        EmptyGraphNeighborhoodReader,
        EmptyNodeDetailReader,
        NoopSnapshotStore,
        InMemoryContextEventStore::new(),
        Arc::new(NoopQualityObserver),
    );

    let get_context_err = server
        .query_application()
        .get_context(GetContextQuery {
            root_node_id: "node-123".to_string(),
            role: "developer".to_string(),
            depth: 0,
            requested_scopes: vec!["graph".to_string()],
            render_options: Default::default(),
        })
        .await;
    assert!(
        get_context_err.is_err(),
        "empty graph should return NotFound"
    );

    let update = server
        .command_application()
        .update_context(rehydration_application::UpdateContextCommand {
            root_node_id: "node-123".to_string(),
            role: "developer".to_string(),
            work_item_id: String::new(),
            changes: Vec::new(),
            expected_revision: None,
            expected_content_hash: None,
            idempotency_key: None,
            logical_digest: None,
            requested_by: None,
        })
        .await
        .expect("command application should respond");
    assert_eq!(update.accepted_version.revision, 1);
}

#[tokio::test]
async fn query_service_returns_rendered_context() {
    let application = Arc::new(QueryApplicationService::new(
        Arc::new(EmptyGraphNeighborhoodReader),
        Arc::new(EmptyNodeDetailReader),
        Arc::new(NoopSnapshotStore),
        "0.1.0",
    ));
    let service = QueryGrpcServiceV1Beta1::new(application, Arc::new(NoopQualityObserver));

    let status = service
        .get_context(Request::new(GetContextRequest {
            root_node_id: "node-123".to_string(),
            role: "developer".to_string(),
            token_budget: 1024,
            requested_scopes: vec!["graph".to_string()],
            depth: 0,
            max_tier: 0,
            rehydration_mode: 0,
        }))
        .await
        .expect_err("empty graph should return NOT_FOUND");
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn query_service_forwards_requested_depth_to_application() {
    let depths = Arc::new(Mutex::new(Vec::new()));
    let application = Arc::new(QueryApplicationService::new(
        Arc::new(RecordingGraphNeighborhoodReader {
            depths: Arc::clone(&depths),
        }),
        Arc::new(EmptyNodeDetailReader),
        Arc::new(NoopSnapshotStore),
        "0.1.0",
    ));
    let service = QueryGrpcServiceV1Beta1::new(application, Arc::new(NoopQualityObserver));

    let _ = service
        .get_context(Request::new(GetContextRequest {
            root_node_id: "node-123".to_string(),
            role: "developer".to_string(),
            token_budget: 1024,
            requested_scopes: vec!["graph".to_string()],
            depth: 17,
            max_tier: 0,
            rehydration_mode: 0,
        }))
        .await;

    assert_eq!(&*depths.lock().await, &[17]);
}

#[tokio::test]
async fn query_service_returns_context_path_bundle() {
    let application = Arc::new(QueryApplicationService::new(
        Arc::new(SeededGraphNeighborhoodReader),
        Arc::new(SeededNodeDetailReader),
        Arc::new(NoopSnapshotStore),
        "0.1.0",
    ));
    let service = QueryGrpcServiceV1Beta1::new(application, Arc::new(NoopQualityObserver));

    let response = service
        .get_context_path(Request::new(GetContextPathRequest {
            root_node_id: "node-123".to_string(),
            target_node_id: "node-789".to_string(),
            role: "developer".to_string(),
            token_budget: 1024,
        }))
        .await
        .expect("get context path should succeed")
        .into_inner();

    let bundle = response.path_bundle.expect("path bundle should exist");
    let role_bundle = bundle
        .bundles
        .first()
        .expect("path bundle should contain one role bundle");

    assert_eq!(bundle.root_node_id, "node-123");
    assert_eq!(role_bundle.neighbor_nodes.len(), 2);
    assert_eq!(role_bundle.relationships.len(), 2);
    assert_eq!(role_bundle.node_details.len(), 3);
    assert!(
        response
            .rendered
            .as_ref()
            .expect("rendered context should exist")
            .content
            .contains("Target detail")
    );
    assert!(response.served_at.is_some());
}

#[tokio::test]
async fn query_service_returns_multi_resolution_tiers_in_rendered_context() {
    let application = Arc::new(QueryApplicationService::new(
        Arc::new(SeededGraphNeighborhoodReader),
        Arc::new(SeededNodeDetailReader),
        Arc::new(NoopSnapshotStore),
        "0.1.0",
    ));
    let service = QueryGrpcServiceV1Beta1::new(application, Arc::new(NoopQualityObserver));

    let response = service
        .get_context_path(Request::new(GetContextPathRequest {
            root_node_id: "node-123".to_string(),
            target_node_id: "node-789".to_string(),
            role: "developer".to_string(),
            token_budget: 4096,
        }))
        .await
        .expect("should succeed")
        .into_inner();

    let rendered = response.rendered.expect("rendered should exist");

    // Flat content still works (backward compat)
    assert!(!rendered.content.is_empty());
    assert!(rendered.token_count > 0);
    assert!(!rendered.sections.is_empty());

    // Tiers are populated
    assert!(
        !rendered.tiers.is_empty(),
        "tiers should be populated in the gRPC response"
    );

    // L0 Summary is always first
    let l0 = &rendered.tiers[0];
    assert_eq!(l0.tier, ResolutionTier::L0Summary as i32);
    assert!(l0.content.contains("Objective:"));
    assert!(l0.token_count > 0);

    // At least L0 + L1 present
    assert!(
        rendered.tiers.len() >= 2,
        "should have at least L0 and L1, got {}",
        rendered.tiers.len()
    );
}

#[tokio::test]
async fn query_service_returns_node_detail_panel() {
    let application = Arc::new(QueryApplicationService::new(
        Arc::new(SeededGraphNeighborhoodReader),
        Arc::new(SeededNodeDetailReader),
        Arc::new(NoopSnapshotStore),
        "0.1.0",
    ));
    let service = QueryGrpcServiceV1Beta1::new(application, Arc::new(NoopQualityObserver));

    let response = service
        .get_node_detail(Request::new(GetNodeDetailRequest {
            node_id: "node-123".to_string(),
        }))
        .await
        .expect("get node detail should succeed")
        .into_inner();

    let node = response.node.expect("node should exist");
    let detail = response.detail.expect("detail should exist");

    assert_eq!(node.node_id, "node-123");
    assert_eq!(node.title, "Node node-123");
    assert_eq!(node.properties["owner"], "ops");
    assert_eq!(detail.detail, "Expanded detail");
    assert_eq!(detail.revision, 2);
}

#[tokio::test]
async fn query_service_returns_node_metadata_when_detail_is_missing() {
    let application = Arc::new(QueryApplicationService::new(
        Arc::new(SeededGraphNeighborhoodReader),
        Arc::new(EmptyNodeDetailReader),
        Arc::new(NoopSnapshotStore),
        "0.1.0",
    ));
    let service = QueryGrpcServiceV1Beta1::new(application, Arc::new(NoopQualityObserver));

    let response = service
        .get_node_detail(Request::new(GetNodeDetailRequest {
            node_id: "graph-only".to_string(),
        }))
        .await
        .expect("graph-only node detail should succeed")
        .into_inner();

    assert_eq!(
        response.node.expect("node should exist").node_id,
        "graph-only"
    );
    assert!(response.detail.is_none());
}

#[tokio::test]
async fn query_service_returns_not_found_for_missing_node_detail_target() {
    let application = Arc::new(QueryApplicationService::new(
        Arc::new(SeededGraphNeighborhoodReader),
        Arc::new(SeededNodeDetailReader),
        Arc::new(NoopSnapshotStore),
        "0.1.0",
    ));
    let service = QueryGrpcServiceV1Beta1::new(application, Arc::new(NoopQualityObserver));

    let error = service
        .get_node_detail(Request::new(GetNodeDetailRequest {
            node_id: "orphan-detail".to_string(),
        }))
        .await
        .expect_err("orphan detail should map to not found");

    assert_eq!(error.code(), tonic::Code::NotFound);
    assert_eq!(error.message(), "Node not found: orphan-detail");
}

#[tokio::test]
async fn query_service_validates_scope_diffs() {
    let application = Arc::new(QueryApplicationService::new(
        Arc::new(EmptyGraphNeighborhoodReader),
        Arc::new(EmptyNodeDetailReader),
        Arc::new(NoopSnapshotStore),
        "0.1.0",
    ));
    let service = QueryGrpcServiceV1Beta1::new(application, Arc::new(NoopQualityObserver));

    let response = service
        .validate_scope(Request::new(ValidateScopeRequest {
            required_scopes: vec!["graph".to_string()],
            provided_scopes: vec!["details".to_string()],
        }))
        .await
        .expect("validate scope should succeed")
        .into_inner();

    let result = response.result.expect("validation result should exist");
    assert!(!result.allowed);
    assert_eq!(result.missing_scopes, vec!["graph".to_string()]);
}

#[tokio::test]
async fn command_service_accepts_update_context() {
    let event_store = Arc::new(InMemoryContextEventStore::new());
    let service =
        CommandGrpcServiceV1Beta1::new(Arc::new(CommandApplicationService::new(Arc::new(
            rehydration_application::UpdateContextUseCase::new(event_store, "0.1.0"),
        ))));

    let response = service
        .update_context(Request::new(UpdateContextRequest {
            root_node_id: "node-123".to_string(),
            role: "developer".to_string(),
            work_item_id: String::new(),
            changes: vec![ContextChange {
                operation: ContextChangeOperation::Update as i32,
                entity_kind: "node_detail".to_string(),
                entity_id: "node-456".to_string(),
                payload_json: "{\"status\":\"ACTIVE\"}".to_string(),
                reason: "refined".to_string(),
                scopes: vec!["graph".to_string()],
            }],
            metadata: None,
            precondition: None,
        }))
        .await
        .expect("update context should succeed")
        .into_inner();

    assert_eq!(
        response
            .accepted_version
            .as_ref()
            .expect("accepted version should exist")
            .revision,
        1
    );
}

#[tokio::test]
async fn command_service_returns_aborted_on_revision_conflict() {
    let event_store = Arc::new(InMemoryContextEventStore::new());
    let service =
        CommandGrpcServiceV1Beta1::new(Arc::new(CommandApplicationService::new(Arc::new(
            rehydration_application::UpdateContextUseCase::new(event_store, "0.1.0"),
        ))));

    let status = service
        .update_context(Request::new(UpdateContextRequest {
            root_node_id: "node-123".to_string(),
            role: "developer".to_string(),
            work_item_id: String::new(),
            changes: vec![],
            metadata: None,
            precondition: Some(rehydration_proto::v1beta1::RevisionPrecondition {
                expected_revision: 99,
                expected_content_hash: String::new(),
            }),
        }))
        .await
        .expect_err("wrong revision should fail");

    assert_eq!(status.code(), tonic::Code::Aborted);
}

#[tokio::test]
async fn memory_service_ingest_dry_run_validates_without_writing() {
    let event_store = Arc::new(InMemoryContextEventStore::new());
    let service = memory_service_with_store(
        EmptyGraphNeighborhoodReader,
        EmptyNodeDetailReader,
        Arc::clone(&event_store),
    );

    let response = service
        .ingest(Request::new(valid_memory_ingest_request(true)))
        .await
        .expect("dry-run memory ingest should validate")
        .into_inner();

    let memory = response.memory.expect("ingested memory should be present");
    assert_eq!(memory.about, "question:830ce83f");
    assert_eq!(memory.memory_id, "memory:transport-test");
    assert!(!memory.read_after_write_ready);
    assert_eq!(memory.accepted.expect("accepted counts").entries, 1);
    assert_eq!(
        event_store
            .current_revision("question:830ce83f", "memory")
            .await
            .expect("revision read should succeed"),
        0
    );
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.contains("dry_run=true"))
    );
}

#[tokio::test]
async fn memory_service_ingest_uses_shallow_existing_ref_lookup() {
    let depths = Arc::new(Mutex::new(Vec::new()));
    let service = memory_service(
        RecordingGraphNeighborhoodReader {
            depths: Arc::clone(&depths),
        },
        EmptyNodeDetailReader,
    );

    service
        .ingest(Request::new(valid_memory_ingest_request(false)))
        .await
        .expect("memory ingest should succeed");

    assert_eq!(&*depths.lock().await, &[1]);
}

#[tokio::test]
async fn memory_service_wake_and_ask_read_live_context() {
    let service = memory_service(SeededGraphNeighborhoodReader, SeededNodeDetailReader);

    let wake = service
        .wake(Request::new(WakeRequest {
            about: "node-123".to_string(),
            role: "developer".to_string(),
            intent: "resume incident".to_string(),
            budget: Some(MemoryBudget {
                max_entries: 0,
                tokens: 1024,
                depth: 1,
                ..Default::default()
            }),
            dimensions: None,
        }))
        .await
        .expect("wake should read live context")
        .into_inner();
    assert!(wake.summary.contains("Objective:"));
    assert_eq!(wake.wake.expect("wake packet").objective, "resume incident");

    let ask = service
        .ask(Request::new(AskRequest {
            about: "node-123".to_string(),
            question: "What is current?".to_string(),
            budget: Some(MemoryBudget {
                max_entries: 0,
                tokens: 1024,
                depth: 1,
                ..Default::default()
            }),
            ..Default::default()
        }))
        .await
        .expect("ask should read live context")
        .into_inner();

    assert_eq!(ask.answer, "UNKNOWN");
    assert!(ask.summary.contains("No deterministic memory answer"));
    assert!(ask.because.is_empty());
    assert!(ask.warnings.is_empty());
}

#[tokio::test]
async fn memory_service_ask_uses_explicit_memory_evidence_not_anchor_detail() {
    let service = memory_service(SeededGraphNeighborhoodReader, SeededNodeDetailReader);

    let ask = service
        .ask(Request::new(AskRequest {
            about: "question:evidence-answer".to_string(),
            question: "What is the explicit answer?".to_string(),
            answer_policy: AnswerPolicy::EvidenceOrUnknown as i32,
            budget: Some(MemoryBudget {
                max_entries: 0,
                tokens: 1024,
                detail: MemoryDetailLevel::Full as i32,
                depth: 3,
            }),
            dimensions: Some(ProtoDimensionSelection {
                mode: ProtoDimensionSelectionMode::Only as i32,
                include: vec!["conversation".to_string()],
                exclude: Vec::new(),
                ..Default::default()
            }),
        }))
        .await
        .expect("ask should use explicit evidence")
        .into_inner();

    assert_eq!(
        ask.answer,
        "Explicit memory evidence is the deterministic Ask answer."
    );
    assert_eq!(ask.because.len(), 1);
    assert_eq!(ask.because[0].r#ref, "detail:evidence:answer");
    assert_eq!(ask.because[0].claim, "evidence:answer");
    let proof = ask.proof.expect("ask proof should be present");
    assert_eq!(proof.evidence.len(), 1);
    assert_eq!(proof.evidence[0].supports, vec!["claim:answer".to_string()]);
    assert!(proof.evidence[0].text.contains("deterministic Ask answer"));
}

#[tokio::test]
async fn memory_service_ask_show_conflicts_surfaces_explicit_conflict_relations() {
    let service = memory_service(SeededGraphNeighborhoodReader, SeededNodeDetailReader);

    let ask = service
        .ask(Request::new(AskRequest {
            about: "question:conflict-answer".to_string(),
            question: "Is cache enabled at the incident checkpoint?".to_string(),
            answer_policy: AnswerPolicy::ShowConflicts as i32,
            budget: Some(MemoryBudget {
                max_entries: 0,
                tokens: 1024,
                detail: MemoryDetailLevel::Full as i32,
                depth: 3,
            }),
            dimensions: Some(ProtoDimensionSelection {
                mode: ProtoDimensionSelectionMode::Only as i32,
                include: vec!["conversation".to_string()],
                exclude: Vec::new(),
                ..Default::default()
            }),
        }))
        .await
        .expect("ask should surface explicit conflicts")
        .into_inner();

    assert_eq!(ask.because.len(), 2);
    assert!(ask.answer.contains("cache=true"));
    assert!(ask.answer.contains("cache=false"));
    let proof = ask.proof.expect("ask proof should be present");
    assert_eq!(
        proof.conflicts,
        vec![
            "claim:cache-enabled contradicts claim:cache-disabled: The cache cannot be both enabled and disabled in the same observed state."
                .to_string()
        ]
    );
    assert!(
        proof
            .path
            .iter()
            .any(|relation| relation.rel == "contradicts")
    );
}

#[tokio::test]
async fn memory_service_wake_and_ask_apply_dimensions_detail_and_answer_policy() {
    let service = memory_service(TemporalGraphNeighborhoodReader, EmptyNodeDetailReader);

    let wake = service
        .wake(Request::new(WakeRequest {
            about: "question:830ce83f".to_string(),
            role: "developer".to_string(),
            intent: "resume relocation memory".to_string(),
            budget: Some(MemoryBudget {
                max_entries: 0,
                tokens: 1024,
                detail: MemoryDetailLevel::Balanced as i32,
                depth: 3,
            }),
            dimensions: Some(ProtoDimensionSelection {
                mode: ProtoDimensionSelectionMode::Only as i32,
                include: vec!["conversation".to_string()],
                exclude: Vec::new(),
                ..Default::default()
            }),
        }))
        .await
        .expect("wake should apply dimension selection")
        .into_inner();

    let proof = wake.proof.expect("wake proof should be present");
    assert!(!proof.path.is_empty());
    assert!(
        proof.path.iter().any(|relationship| relationship.source_ref
            == "about:question:830ce83f:dimension:conversation")
    );
    assert!(
        proof
            .path
            .iter()
            .all(|relationship| relationship.source_ref != "person:rachel"
                && relationship.source_ref != "longmemeval:item:830ce83f")
    );
    assert_eq!(
        wake.wake.expect("wake packet should be present").objective,
        "resume relocation memory"
    );

    let ask = service
        .ask(Request::new(AskRequest {
            about: "question:830ce83f".to_string(),
            question: "Where did Rachel move?".to_string(),
            answer_policy: AnswerPolicy::EvidenceOrUnknown as i32,
            budget: Some(MemoryBudget {
                max_entries: 0,
                tokens: 1024,
                detail: MemoryDetailLevel::Compact as i32,
                depth: 3,
            }),
            dimensions: Some(ProtoDimensionSelection {
                mode: ProtoDimensionSelectionMode::Only as i32,
                include: vec!["benchmark_record".to_string()],
                exclude: Vec::new(),
                ..Default::default()
            }),
        }))
        .await
        .expect("ask should apply answer policy")
        .into_inner();

    assert_eq!(ask.answer, "UNKNOWN");
    assert_eq!(
        ask.proof.expect("ask proof should be present").confidence,
        MemoryConfidence::Unknown as i32
    );
}

#[tokio::test]
async fn memory_service_temporal_methods_use_domain_traversal() {
    let service = memory_service(TemporalGraphNeighborhoodReader, EmptyNodeDetailReader);

    let goto = service
        .goto(Request::new(goto_request(
            Some(ProtoTemporalCursor {
                time: Some(ts(103)),
                ..Default::default()
            }),
            ProtoDimensionSelection::default(),
        )))
        .await
        .expect("goto should traverse temporal memory")
        .into_inner();
    assert_eq!(
        goto.temporal.expect("temporal state").direction,
        ProtoTemporalDirection::Goto as i32
    );
    assert_eq!(goto.entries[0].r#ref, "claim:rachel-denver");

    let near = service
        .near(Request::new(NearRequest {
            about: "question:830ce83f".to_string(),
            around: Some(ProtoTemporalCursor {
                time: Some(ts(103)),
                ..Default::default()
            }),
            window: Some(ProtoTemporalWindow {
                before_entries: 1,
                after_entries: 1,
            }),
            limit: Some(TemporalLimit {
                entries: 5,
                tokens: 0,
            }),
            include: Some(TemporalInclude {
                evidence: false,
                relations: true,
                raw_refs: false,
            }),
            ..Default::default()
        }))
        .await
        .expect("near should traverse temporal memory")
        .into_inner();
    assert_eq!(
        near.entries
            .iter()
            .map(|entry| entry.r#ref.as_str())
            .collect::<Vec<_>>(),
        vec!["claim:rachel-denver", "claim:rachel-austin"]
    );
    assert!(
        !near
            .proof
            .expect("temporal proof should be present")
            .path
            .is_empty()
    );

    let rewind = service
        .rewind(Request::new(rewind_request(
            Some(ProtoTemporalCursor {
                sequence: Some(7),
                ..Default::default()
            }),
            ProtoDimensionSelection::default(),
        )))
        .await
        .expect("rewind should traverse sequence positions")
        .into_inner();
    assert_eq!(
        rewind
            .entries
            .iter()
            .map(|entry| entry.r#ref.as_str())
            .collect::<Vec<_>>(),
        vec!["claim:rachel-denver", "claim:rachel-austin"]
    );

    let forward = service
        .forward(Request::new(forward_request(
            Some(ProtoTemporalCursor {
                r#ref: "claim:rachel-denver".to_string(),
                ..Default::default()
            }),
            ProtoDimensionSelection {
                mode: ProtoDimensionSelectionMode::Only as i32,
                include: vec!["conversation".to_string()],
                exclude: Vec::new(),
                ..Default::default()
            },
        )))
        .await
        .expect("forward should respect dimension selection")
        .into_inner();
    assert_eq!(forward.entries[0].r#ref, "claim:rachel-austin");
    let forward_coverage = forward.coverage.expect("coverage");
    assert_eq!(forward_coverage.included, ["conversation"]);
    let requested = forward_coverage.requested.expect("requested");
    assert_eq!(
        requested.scope,
        ProtoDimensionScopeMode::CurrentAbout as i32
    );
    assert!(requested.abouts.is_empty());

    let all_abouts_forward = service
        .forward(Request::new(forward_request(
            Some(ProtoTemporalCursor {
                r#ref: "claim:rachel-denver".to_string(),
                ..Default::default()
            }),
            ProtoDimensionSelection {
                mode: ProtoDimensionSelectionMode::Only as i32,
                include: vec!["conversation".to_string()],
                exclude: Vec::new(),
                scope: ProtoDimensionScopeMode::AllAbouts as i32,
                abouts: Vec::new(),
                ..Default::default()
            },
        )))
        .await
        .expect("ALL_ABOUTS should traverse every indexed memory about")
        .into_inner();
    assert_eq!(
        all_abouts_forward
            .entries
            .iter()
            .map(|entry| entry.r#ref.as_str())
            .collect::<Vec<_>>(),
        vec!["claim:rachel-austin", "claim:porto-lisbon"]
    );
    let all_abouts_coverage = all_abouts_forward.coverage.expect("coverage");
    assert_eq!(
        all_abouts_coverage.requested.expect("requested").scope,
        ProtoDimensionScopeMode::AllAbouts as i32
    );

    let exact_scope_forward = service
        .forward(Request::new(forward_request(
            Some(ProtoTemporalCursor {
                r#ref: "claim:rachel-denver".to_string(),
                ..Default::default()
            }),
            ProtoDimensionSelection {
                mode: ProtoDimensionSelectionMode::Only as i32,
                include: vec!["conversation".to_string()],
                exclude: Vec::new(),
                scope: ProtoDimensionScopeMode::AllAbouts as i32,
                abouts: Vec::new(),
                scope_ids: vec!["about:question:830ce83f:dimension:conversation".to_string()],
            },
        )))
        .await
        .expect("exact scope_ids should narrow ALL_ABOUTS traversal")
        .into_inner();
    assert_eq!(
        exact_scope_forward
            .entries
            .iter()
            .map(|entry| entry.r#ref.as_str())
            .collect::<Vec<_>>(),
        vec!["claim:rachel-austin"]
    );
    let exact_coverage = exact_scope_forward.coverage.expect("coverage");
    assert_eq!(
        exact_coverage.requested.expect("requested").scope_ids,
        vec!["about:question:830ce83f:dimension:conversation".to_string()]
    );
}

#[tokio::test]
async fn memory_service_temporal_raw_refs_return_typed_audit_refs() {
    let service = memory_service(TemporalGraphNeighborhoodReader, EmptyNodeDetailReader);
    let mut request = temporal_move_request(
        Some(ProtoTemporalCursor {
            r#ref: "claim:rachel-denver".to_string(),
            ..Default::default()
        }),
        ProtoDimensionSelection::default(),
    );
    request.include = Some(TemporalInclude {
        evidence: false,
        relations: false,
        raw_refs: true,
    });

    let raw_refs = service
        .forward(Request::new(forward_request_from_temporal(request)))
        .await
        .expect("temporal raw_refs should return typed refs")
        .into_inner();

    assert_eq!(raw_refs.raw_refs.len(), 1);
    assert_eq!(raw_refs.raw_refs[0].r#ref, "claim:rachel-austin");
    assert_eq!(raw_refs.raw_refs[0].coordinates[0].sequence, Some(2));
}

#[tokio::test]
async fn memory_service_temporal_requests_fail_fast_on_ambiguous_inputs() {
    let service = memory_service(TemporalGraphNeighborhoodReader, EmptyNodeDetailReader);

    let ambiguous_cursor = service
        .forward(Request::new(forward_request(
            Some(ProtoTemporalCursor {
                r#ref: "claim:rachel-denver".to_string(),
                sequence: Some(1),
                ..Default::default()
            }),
            ProtoDimensionSelection::default(),
        )))
        .await
        .expect_err("ambiguous cursor should fail");
    assert_eq!(ambiguous_cursor.code(), tonic::Code::InvalidArgument);

    let empty_only_selection = service
        .goto(Request::new(goto_request(
            Some(ProtoTemporalCursor {
                sequence: Some(1),
                ..Default::default()
            }),
            ProtoDimensionSelection {
                mode: ProtoDimensionSelectionMode::Only as i32,
                include: Vec::new(),
                exclude: Vec::new(),
                ..Default::default()
            },
        )))
        .await
        .expect_err("empty ONLY selection should fail");
    assert_eq!(empty_only_selection.code(), tonic::Code::InvalidArgument);

    let empty_about_scope = service
        .goto(Request::new(goto_request(
            Some(ProtoTemporalCursor {
                sequence: Some(1),
                ..Default::default()
            }),
            ProtoDimensionSelection {
                mode: ProtoDimensionSelectionMode::Only as i32,
                include: vec!["conversation".to_string()],
                exclude: Vec::new(),
                scope: ProtoDimensionScopeMode::Abouts as i32,
                abouts: Vec::new(),
                ..Default::default()
            },
        )))
        .await
        .expect_err("ABOUTS scope without abouts should fail");
    assert_eq!(empty_about_scope.code(), tonic::Code::InvalidArgument);

    let zero_sequence_cursor = service
        .goto(Request::new(goto_request(
            Some(ProtoTemporalCursor {
                sequence: Some(0),
                ..Default::default()
            }),
            ProtoDimensionSelection::default(),
        )))
        .await
        .expect_err("zero sequence cursor should fail");
    assert_eq!(zero_sequence_cursor.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn memory_service_trace_and_inspect_use_existing_query_ports() {
    let context_path_depths = Arc::new(Mutex::new(Vec::new()));
    let service = memory_service(
        RecordingSeededGraphNeighborhoodReader {
            context_path_depths: Arc::clone(&context_path_depths),
        },
        SeededNodeDetailReader,
    );

    let trace = service
        .trace(Request::new(TraceRequest {
            from: "node-123".to_string(),
            to: "node-789".to_string(),
            goal: "prove path".to_string(),
            budget: Some(MemoryBudget {
                max_entries: 0,
                tokens: 1024,
                depth: 1,
                ..Default::default()
            }),
            page: None,
        }))
        .await
        .expect("trace should read context path")
        .into_inner();
    assert_eq!(trace.trace.len(), 2);
    assert_eq!(trace.trace[0].source_ref, "node-123");

    let paged_trace = service
        .trace(Request::new(TraceRequest {
            from: "node-123".to_string(),
            to: "node-789".to_string(),
            goal: "prove path".to_string(),
            budget: Some(MemoryBudget {
                max_entries: 0,
                tokens: 1024,
                depth: 1,
                ..Default::default()
            }),
            page: Some(PageRequest {
                entries: 1,
                cursor: String::new(),
            }),
        }))
        .await
        .expect("paged trace should read context path")
        .into_inner();
    assert_eq!(paged_trace.trace.len(), 1);
    let page = paged_trace.page.expect("page info should be returned");
    assert_eq!(page.returned, 1);
    assert_eq!(page.total, 2);
    assert!(page.has_more);
    assert_eq!(page.next_cursor, "1");
    assert_eq!(&*context_path_depths.lock().await, &[0, 0]);

    let invalid_cursor = service
        .trace(Request::new(TraceRequest {
            from: "node-123".to_string(),
            to: "node-789".to_string(),
            goal: "prove path".to_string(),
            budget: None,
            page: Some(PageRequest {
                entries: 1,
                cursor: "not-a-trace-cursor".to_string(),
            }),
        }))
        .await
        .expect_err("invalid trace page cursor should fail fast");
    assert_eq!(invalid_cursor.code(), tonic::Code::InvalidArgument);

    let oversized_page = service
        .trace(Request::new(TraceRequest {
            from: "node-123".to_string(),
            to: "node-789".to_string(),
            goal: "prove path".to_string(),
            budget: None,
            page: Some(PageRequest {
                entries: MAX_TRACE_PAGE_ENTRIES as u32 + 1,
                cursor: String::new(),
            }),
        }))
        .await
        .expect_err("oversized trace page should fail fast");
    assert_eq!(oversized_page.code(), tonic::Code::InvalidArgument);

    let inspect = service
        .inspect(Request::new(InspectRequest {
            r#ref: "node-123".to_string(),
            include: Some(InspectInclude {
                incoming: true,
                outgoing: true,
                details: true,
                raw: false,
            }),
        }))
        .await
        .expect("inspect should read node detail and direct links")
        .into_inner();
    assert_eq!(inspect.object.expect("object").r#ref, "node-123");
    assert_eq!(inspect.evidence.len(), 1);
    let links = inspect.links.expect("links");
    assert_eq!(links.incoming.len(), 2);
    assert!(
        links
            .incoming
            .iter()
            .any(|relationship| relationship.source_ref == "node-000")
    );
    assert_eq!(links.outgoing.len(), 1);
    assert_eq!(links.outgoing[0].target_ref, "node-456");

    let summary_only = service
        .inspect(Request::new(InspectRequest {
            r#ref: "node-123".to_string(),
            include: Some(InspectInclude {
                incoming: false,
                outgoing: false,
                details: false,
                raw: false,
            }),
        }))
        .await
        .expect("inspect should honor details=false")
        .into_inner();
    assert_eq!(
        summary_only.object.expect("object").text,
        "Summary for node-123"
    );
    assert!(summary_only.evidence.is_empty());

    let raw = service
        .inspect(Request::new(InspectRequest {
            r#ref: "node-123".to_string(),
            include: Some(InspectInclude {
                incoming: false,
                outgoing: false,
                details: true,
                raw: true,
            }),
        }))
        .await
        .expect("inspect raw should return typed audit refs")
        .into_inner();

    assert_eq!(raw.raw.len(), 1);
    assert_eq!(raw.raw[0].r#ref, "node-123");
    assert_eq!(raw.raw[0].detail, "Expanded detail");
    assert_eq!(raw.raw[0].revision, 2);
    assert_eq!(raw.raw[0].coordinates.len(), 1);
    assert_eq!(raw.raw[0].coordinates[0].dimension, "conversation");
    assert_eq!(
        raw.raw[0].coordinates[0].scope_id,
        "about:node-123:dimension:conversation"
    );
    assert_eq!(raw.raw[0].coordinates[0].sequence, Some(4));
}

#[test]
fn helper_mappers_cover_bundle_mapping() {
    let bundle = sample_bundle("node-123", "developer", "Projection-backed summary");
    let proto_bundle = proto_bundle_from_single_role_v1beta1(&bundle);
    let proto_root = proto_bundle_node_v1beta1(bundle.root_node());
    let proto_relationship = proto_bundle_relationship_v1beta1(&bundle.relationships()[0]);
    let proto_detail = proto_bundle_node_detail_v1beta1(&bundle.node_details()[0]);

    assert_eq!(proto_bundle.root_node_id, "node-123");
    assert_eq!(proto_bundle.bundles.len(), 1);
    assert_eq!(proto_bundle.bundles[0].role, "developer");
    assert_eq!(
        proto_bundle.bundles[0]
            .root_node
            .as_ref()
            .expect("root node should exist")
            .summary,
        "Projection-backed summary"
    );
    assert_eq!(proto_bundle.bundles[0].neighbor_nodes.len(), 1);
    assert_eq!(proto_bundle.bundles[0].relationships.len(), 1);
    assert_eq!(proto_bundle.bundles[0].node_details.len(), 1);
    assert_eq!(
        proto_bundle
            .stats
            .as_ref()
            .expect("stats should exist")
            .nodes,
        2
    );
    assert_eq!(proto_root.status, "ACTIVE");
    assert_eq!(proto_relationship.relationship_type, "RELATES_TO");
    assert_eq!(proto_detail.content_hash, "hash-1");
}

#[test]
fn helper_mappers_cover_versions_errors_and_trim_logic() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_100);
    let version = super::proto_mapping_v1beta1::proto_bundle_version_v1beta1(&BundleMetadata {
        revision: 7,
        content_hash: "abc123".to_string(),
        generator_version: "0.1.0".to_string(),
    });
    let accepted = proto_accepted_version_v1beta1(&AcceptedVersion {
        revision: 8,
        content_hash: "xyz789".to_string(),
        generator_version: "0.1.0".to_string(),
    });

    assert_eq!(version.revision, 7);
    assert_eq!(accepted.projection_watermark, "rev-8");
    assert_eq!(proto_duration(30).seconds, 30);
    assert_eq!(
        trim_to_option("  value  ".to_string()),
        Some("value".to_string())
    );
    assert_eq!(trim_to_option("   ".to_string()), None);
    assert_eq!(
        map_application_error(ApplicationError::Validation("bad".to_string())).code(),
        tonic::Code::InvalidArgument
    );
    assert_eq!(
        map_application_error(ApplicationError::Ports(PortError::Unavailable(
            "down".to_string()
        )))
        .code(),
        tonic::Code::Unavailable
    );
    assert_eq!(
        map_application_error(ApplicationError::NotFound("missing".to_string())).code(),
        tonic::Code::NotFound
    );
    let session_bundle = sample_bundle("node-123", "developer", "Section one");
    let rendered_contexts = vec![rehydration_application::render_graph_bundle_with_options(
        &session_bundle,
        &Default::default(),
    )];
    let response = proto_rehydrate_session_response_v1beta1(&RehydrateSessionResult {
        root_node_id: "node-123".to_string(),
        bundles: vec![session_bundle],
        rendered_contexts,
        timeline_events: 9,
        version: BundleMetadata::initial("0.1.0"),
        snapshot_persisted: true,
        snapshot_id: Some("snapshot:node-123:developer".to_string()),
        generated_at: now,
        timing: None,
    });
    assert_eq!(
        response
            .bundle
            .expect("bundle should exist")
            .stats
            .expect("stats should exist")
            .timeline_events,
        9
    );

    let invalid_argument = map_application_error(ApplicationError::Domain(
        rehydration_domain::DomainError::EmptyValue("root_node_id"),
    ));
    assert_eq!(invalid_argument.code(), tonic::Code::InvalidArgument);
}

fn valid_memory_ingest_request(dry_run: bool) -> IngestRequest {
    IngestRequest {
        about: "question:830ce83f".to_string(),
        memory: Some(Memory {
            dimensions: vec![MemoryDimension {
                id: "conversation:rachel-2026-04-12".to_string(),
                kind: "conversation".to_string(),
                title: "Rachel relocation discussion".to_string(),
                metadata: Default::default(),
            }],
            entries: vec![MemoryEntry {
                id: "claim:rachel-denver".to_string(),
                kind: "claim".to_string(),
                text: "Rachel said she was moving to Denver.".to_string(),
                coordinates: vec![ProtoTemporalCoordinate {
                    dimension: "conversation".to_string(),
                    scope_id: "conversation:rachel-2026-04-12".to_string(),
                    occurred_at: Some(ts(100)),
                    sequence: Some(1),
                    ..Default::default()
                }],
                metadata: Default::default(),
            }],
            relations: vec![MemoryRelation {
                source_ref: "conversation:rachel-2026-04-12".to_string(),
                target_ref: "claim:rachel-denver".to_string(),
                rel: "contains_entry".to_string(),
                semantic_class: MemorySemanticClass::Structural as i32,
                confidence: MemoryConfidence::Medium as i32,
                sequence: Some(1),
                ..Default::default()
            }],
            evidence: vec![MemoryEvidence {
                id: "evidence:rachel-denver".to_string(),
                supports: vec!["claim:rachel-denver".to_string()],
                text: "Conversation transcript line 1".to_string(),
                source: "transcript:1".to_string(),
                time: Some(ts(100)),
                metadata: Default::default(),
            }],
        }),
        provenance: Some(MemoryProvenance {
            source_kind: MemorySourceKind::Human as i32,
            source_agent: "transport-test".to_string(),
            observed_at: Some(ts(101)),
            correlation_id: "corr-1".to_string(),
            causation_id: String::new(),
        }),
        idempotency_key: "ingest:transport-test".to_string(),
        dry_run,
    }
}

fn temporal_move_request(
    cursor: Option<ProtoTemporalCursor>,
    dimensions: ProtoDimensionSelection,
) -> TemporalMoveRequest {
    TemporalMoveRequest {
        about: "question:830ce83f".to_string(),
        cursor,
        dimensions: Some(dimensions),
        window: Some(ProtoTemporalWindow {
            before_entries: 1,
            after_entries: 1,
        }),
        limit: Some(TemporalLimit {
            entries: 5,
            tokens: 0,
        }),
        include: None,
        budget: Some(MemoryBudget {
            max_entries: 0,
            tokens: 1024,
            depth: 3,
            ..Default::default()
        }),
    }
}

fn goto_request(
    cursor: Option<ProtoTemporalCursor>,
    dimensions: ProtoDimensionSelection,
) -> GotoRequest {
    let request = temporal_move_request(cursor, dimensions);
    GotoRequest {
        about: request.about,
        cursor: request.cursor,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
    }
}

fn rewind_request(
    cursor: Option<ProtoTemporalCursor>,
    dimensions: ProtoDimensionSelection,
) -> RewindRequest {
    let request = temporal_move_request(cursor, dimensions);
    RewindRequest {
        about: request.about,
        cursor: request.cursor,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
    }
}

fn forward_request(
    cursor: Option<ProtoTemporalCursor>,
    dimensions: ProtoDimensionSelection,
) -> ForwardRequest {
    forward_request_from_temporal(temporal_move_request(cursor, dimensions))
}

fn forward_request_from_temporal(request: TemporalMoveRequest) -> ForwardRequest {
    ForwardRequest {
        about: request.about,
        cursor: request.cursor,
        dimensions: request.dimensions,
        window: request.window,
        limit: request.limit,
        include: request.include,
        budget: request.budget,
    }
}

fn temporal_projection(node_id: &str, kind: &str, summary: &str) -> NodeProjection {
    NodeProjection {
        node_id: node_id.to_string(),
        node_kind: kind.to_string(),
        title: summary.to_string(),
        summary: summary.to_string(),
        status: "ACTIVE".to_string(),
        labels: vec![kind.to_string()],
        properties: BTreeMap::new(),
        provenance: None,
    }
}

fn temporal_contains_entry(
    scope_id: &str,
    entry_id: &str,
    dimension: &str,
    sequence: u32,
    occurred_at: Option<String>,
    valid_from: Option<String>,
    rank: Option<u32>,
) -> NodeRelationProjection {
    NodeRelationProjection {
        source_node_id: scope_id.to_string(),
        target_node_id: entry_id.to_string(),
        relation_type: "contains_entry".to_string(),
        explanation: RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension(dimension)
            .with_scope_id(scope_id)
            .with_optional_occurred_at(occurred_at)
            .with_optional_valid_from(valid_from)
            .with_sequence(sequence)
            .with_optional_rank(rank),
    }
}

fn ts(seconds: i64) -> Timestamp {
    Timestamp { seconds, nanos: 0 }
}

fn sort_time(seconds: i64) -> String {
    format!("unix:{:012}:000000000", seconds + 100_000_000_000)
}

fn sample_bundle(root_node_id: &str, role: &str, summary: &str) -> RehydrationBundle {
    let case_id = CaseId::new(root_node_id).expect("root node id is valid");
    let role = Role::new(role).expect("role is valid");

    RehydrationBundle::new(
        case_id.clone(),
        role,
        BundleNode::new(
            case_id.as_str(),
            "capability",
            format!("Node {}", case_id.as_str()),
            summary,
            "ACTIVE",
            vec!["projection-node".to_string()],
            BTreeMap::new(),
        ),
        vec![BundleNode::new(
            "node-456",
            "artifact",
            "Linked artifact",
            "Linked summary",
            "ACTIVE",
            vec!["artifact".to_string()],
            BTreeMap::new(),
        )],
        vec![BundleRelationship::new(
            case_id.as_str(),
            "node-456",
            "RELATES_TO",
            RelationExplanation::new(RelationSemanticClass::Structural),
        )],
        vec![BundleNodeDetail::new(
            case_id.as_str(),
            summary,
            "hash-1",
            1,
        )],
        BundleMetadata::initial("0.1.0"),
    )
    .expect("sample bundle should be valid")
}

fn sample_node_neighborhood(node_id: &str, status: &str) -> NodeNeighborhood {
    NodeNeighborhood {
        root: rehydration_domain::NodeProjection {
            node_id: node_id.to_string(),
            node_kind: "task".to_string(),
            title: format!("Node {node_id}"),
            summary: format!("Summary for {node_id}"),
            status: status.to_string(),
            labels: vec!["Task".to_string()],
            properties: [("owner".to_string(), "ops".to_string())]
                .into_iter()
                .collect(),
            provenance: None,
        },
        neighbors: Vec::new(),
        relations: Vec::new(),
    }
}
