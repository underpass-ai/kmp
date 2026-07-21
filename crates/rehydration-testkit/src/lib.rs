mod container_runtime;
pub mod dataset_generator;
mod graph_batch_llm;
mod in_memory_kernel_store;
mod in_memory_stores;
pub mod interpretation_plugins;
pub mod kernel_operator;
pub mod llm_evaluator;
mod llm_graph;
pub mod longmemeval;
pub mod longmemeval_smart_writer;
pub mod memoryagentbench;
pub mod memoryarena;
pub mod memoryarena_scorecard;
pub mod memoryarena_smart_writer;
pub mod raw_dump;
pub mod seed_publisher;
pub mod seed_to_bundle;
pub mod text_normalization;

pub use container_runtime::ensure_testcontainers_runtime;
pub use dataset_generator::{
    Domain, GeneratedNode, GeneratedRelation, GeneratedSeed, GraphSeedConfig, NoiseMode,
    RelationMix, generate_seed,
};
pub use graph_batch_llm::{
    GraphBatchRepairJudgePolicy, GraphBatchRequestError, GraphBatchRequestOutcome,
    GraphBatchRetryPolicy, GraphBatchSemanticClassifierOutcome, GraphBatchSemanticClassifierPolicy,
    build_graph_batch_request_body, classify_graph_batch_semantic_classes_with_policy,
    request_graph_batch_with_policy, request_graph_batch_with_repair_judge,
    request_graph_batch_with_retry,
};
pub use in_memory_kernel_store::InMemoryKernelStore;
pub use in_memory_stores::{
    InMemoryContextEventStore, InMemoryGraphNeighborhoodReader, InMemoryNodeDetailReader,
    InMemoryProcessedEventStore, InMemoryProjectionCheckpointStore, InMemoryProjectionWriter,
    NoopSnapshotStore,
};
pub use interpretation_plugins::{
    CalendarDate, CurrencyCode, CurrencyDerivationPlugin, DateDerivationPlugin, DateValuePlugin,
    DerivationOperand, DerivationOperation, DerivationRequest, DerivationResult, EvidenceFragment,
    EvidenceInterpretationInput, EvidenceInterpretationOutput, EvidenceSegmentKind,
    EvidenceValuePlugin, InterpretationError, InterpretedValue, InterpretedValueMention,
    MathExpressionNotation, MathExpressionValuePlugin, MoneyValuePlugin, OperandLabel, OperandRole,
    SourceCodeSegmentKind, SourceCodeValuePlugin, TextSpan, UrlValuePlugin, ValueOperationPlugin,
};
pub use kernel_operator::{
    kernel_operator_action_contract_error, kernel_operator_action_shape_error,
    kernel_operator_allowed_full_tools, kernel_operator_allowed_read_tools,
    kernel_operator_allowed_write_tools, kernel_operator_is_bounded_tool_call,
    kernel_operator_is_valid_action_shape, kernel_operator_primary_refs,
};
pub use llm_evaluator::{
    CalibrationCase, EvaluationGroundTruth, LlmEvaluationResult, LlmEvaluatorConfig, LlmProvider,
    LlmSemanticClassifierMode, PromptConfig, calibrate_agent, calibrate_judge, call_llm,
    evaluate_with_config, evaluate_with_llm, normalize_llm_json_response,
};
pub use llm_graph::{
    GraphBatch, GraphBatchNode, GraphBatchNodeDetail, GraphBatchRelation, LlmGraphBatch,
    LlmGraphError, LlmGraphNode, LlmGraphRelation, LlmNodeDetail, ProjectionMessage,
    graph_batch_to_projection_events, llm_graph_to_projection_events, namespace_graph_batch,
    parse_graph_batch, parse_llm_graph_batch,
};
pub use longmemeval::{
    LongMemEvalAdapterConfig, LongMemEvalAdapterError, LongMemEvalAdapterSummary,
    LongMemEvalCandidateTurn, LongMemEvalEvidenceLabels, LongMemEvalEvidenceTurnLabel,
    LongMemEvalExpected, LongMemEvalItem, LongMemEvalPreparedItem, LongMemEvalTurn,
    longmemeval_answer_turn_refs, longmemeval_candidate_turns, longmemeval_ref_scope,
    longmemeval_turn_ref, normalize_longmemeval_date, parse_longmemeval_dataset,
    prepare_longmemeval_item, prepare_longmemeval_items,
};
pub use longmemeval_smart_writer::{
    LongMemEvalSmartWriter, LongMemEvalSmartWriterConfig, LongMemEvalSmartWriterItem,
    LongMemEvalSmartWriterResult, LongMemEvalSmartWriterSummary,
    summarize_longmemeval_smart_writer,
};
pub use memoryagentbench::{
    MemoryAgentBenchAdapterConfig, MemoryAgentBenchAdapterError, MemoryAgentBenchAdapterSummary,
    MemoryAgentBenchAskArtifact, MemoryAgentBenchExpected, MemoryAgentBenchIngestArtifact,
    MemoryAgentBenchItem, MemoryAgentBenchKnownAtSnapshot, MemoryAgentBenchMetadata,
    MemoryAgentBenchPreparedItem, MemoryAgentBenchReplay, MemoryAgentBenchReplayEvent,
    memoryagentbench_ref_scope, parse_memoryagentbench_dataset, prepare_memoryagentbench_item,
    prepare_memoryagentbench_items,
};
pub use memoryarena::{
    MemoryArenaAdapterConfig, MemoryArenaAdapterError, MemoryArenaAdapterSummary,
    MemoryArenaAskArtifact, MemoryArenaExpected, MemoryArenaIngestArtifact, MemoryArenaItem,
    MemoryArenaKnownAtSnapshot, MemoryArenaPreparedTask, MemoryArenaReplay, MemoryArenaReplayEvent,
    memoryarena_answer_ref, memoryarena_question_ref, memoryarena_ref_scope,
    parse_memoryarena_dataset, prepare_memoryarena_item, prepare_memoryarena_items,
};
pub use memoryarena_scorecard::{
    MemoryArenaAnswerScore, memoryarena_answer_candidates_from_text,
    memoryarena_answer_candidates_from_value, memoryarena_answers_match,
    memoryarena_task_success_rule, score_memoryarena_answer,
};
pub use memoryarena_smart_writer::{
    MemoryArenaSmartWriter, MemoryArenaSmartWriterConfig, MemoryArenaSmartWriterEvent,
    MemoryArenaSmartWriterResult, MemoryArenaSmartWriterSummary, detect_provider_from_model,
    parse_provider, summarize_smart_writer,
};
pub use seed_to_bundle::{seed_raw_equivalent_tokens, seed_to_bundle};
pub use text_normalization::{
    DetectedTextKind, DetectedTextSpan, NormalizedText, TextNormalizationPipeline,
};

#[cfg(test)]
fn seed_bundle(
    case_id: rehydration_domain::CaseId,
    role: &str,
) -> rehydration_domain::RehydrationBundle {
    let role = rehydration_domain::Role::new(role).expect("role must be valid");
    rehydration_domain::RehydrationBundle::new(
        case_id.clone(),
        role.clone(),
        rehydration_domain::BundleNode::new(
            case_id.as_str(),
            "capability",
            format!("Node {}", case_id.as_str()),
            format!(
                "bundle for node {} role {}",
                case_id.as_str(),
                role.as_str()
            ),
            "ACTIVE",
            vec!["projection-node".to_string()],
            std::collections::BTreeMap::new(),
        ),
        Vec::new(),
        Vec::new(),
        vec![rehydration_domain::BundleNodeDetail::new(
            case_id.as_str(),
            format!(
                "bundle for node {} role {}",
                case_id.as_str(),
                role.as_str()
            ),
            "pending",
            1,
        )],
        rehydration_domain::BundleMetadata::initial("0.1.0"),
    )
    .expect("seed bundle should be valid")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rehydration_domain::{
        CaseId, GraphNeighborhoodReader, NodeDetailProjection, NodeDetailReader, NodeNeighborhood,
        NodeProjection, ProcessedEventStore, ProjectionCheckpoint, ProjectionCheckpointStore,
        ProjectionMutation, ProjectionWriter, SnapshotStore,
    };

    use super::{
        InMemoryGraphNeighborhoodReader, InMemoryNodeDetailReader, InMemoryProcessedEventStore,
        InMemoryProjectionCheckpointStore, InMemoryProjectionWriter, NoopSnapshotStore,
        seed_bundle,
    };

    #[tokio::test]
    async fn in_memory_graph_reader_returns_seeded_neighborhood() {
        let reader = InMemoryGraphNeighborhoodReader::with_neighborhood(NodeNeighborhood {
            root: NodeProjection {
                node_id: "node-123".to_string(),
                node_kind: "capability".to_string(),
                title: "Projection".to_string(),
                summary: String::new(),
                status: "ACTIVE".to_string(),
                labels: vec!["projection".to_string()],
                properties: BTreeMap::new(),
                provenance: None,
            },
            neighbors: Vec::new(),
            relations: Vec::new(),
        });

        let loaded = reader
            .load_neighborhood("node-123", 1)
            .await
            .expect("load should succeed");

        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn in_memory_node_detail_reader_returns_seeded_detail() {
        let reader = InMemoryNodeDetailReader::with_details([NodeDetailProjection {
            node_id: "node-123".to_string(),
            detail: "Expanded detail".to_string(),
            content_hash: "hash-1".to_string(),
            revision: 1,
        }]);

        let loaded = reader
            .load_node_detail("node-123")
            .await
            .expect("load should succeed");

        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn in_memory_projection_writer_records_mutations() {
        let writer = InMemoryProjectionWriter::default();
        writer
            .apply_mutations(vec![
                ProjectionMutation::UpsertNode(NodeProjection {
                    node_id: "node-123".to_string(),
                    node_kind: "task".to_string(),
                    title: "Task 123".to_string(),
                    summary: "Projection updated".to_string(),
                    status: "ACTIVE".to_string(),
                    labels: vec!["work-item".to_string()],
                    properties: BTreeMap::new(),
                    provenance: None,
                }),
                ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
                    node_id: "node-123".to_string(),
                    detail: "expanded node detail".to_string(),
                    content_hash: "hash-1".to_string(),
                    revision: 1,
                }),
            ])
            .await
            .expect("write should succeed");

        assert_eq!(writer.mutations().await.len(), 2);
    }

    #[tokio::test]
    async fn in_memory_processed_event_store_tracks_deduplication() {
        let store = InMemoryProcessedEventStore::default();
        assert!(
            !store
                .has_processed("context-projection", "evt-1")
                .await
                .expect("lookup should succeed")
        );

        store
            .record_processed("context-projection", "evt-1")
            .await
            .expect("record should succeed");

        assert!(
            store
                .has_processed("context-projection", "evt-1")
                .await
                .expect("lookup should succeed")
        );
        assert_eq!(store.processed().await.len(), 1);
    }

    #[tokio::test]
    async fn in_memory_checkpoint_store_persists_latest_checkpoint() {
        let store = InMemoryProjectionCheckpointStore::default();
        store
            .save_checkpoint(ProjectionCheckpoint {
                consumer_name: "context-projection".to_string(),
                stream_name: "rehydration.events".to_string(),
                last_subject: "graph.node.materialized".to_string(),
                last_event_id: "evt-1".to_string(),
                last_correlation_id: "corr-1".to_string(),
                last_occurred_at: "2026-03-07T00:00:00Z".to_string(),
                processed_events: 1,
                updated_at: std::time::SystemTime::UNIX_EPOCH,
            })
            .await
            .expect("save should succeed");

        let checkpoint = store
            .checkpoint("context-projection", "rehydration.events")
            .await
            .expect("checkpoint should exist");
        assert_eq!(checkpoint.last_event_id, "evt-1");
    }

    #[tokio::test]
    async fn noop_snapshot_store_accepts_bundle() {
        let bundle = seed_bundle(
            CaseId::new("case-123").expect("case id is valid"),
            "developer",
        );

        NoopSnapshotStore
            .save_bundle(&bundle)
            .await
            .expect("noop snapshot store should accept bundles");
    }
}
