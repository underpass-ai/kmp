use super::curate_doubt_cache::CurateDoubtCache;
use super::curate_review_cache::CurateReviewCache;
use super::embedded::{
    EmbeddedAskTool, EmbeddedCondenseTool, EmbeddedCurateTool, EmbeddedIngestTool,
    EmbeddedInspectTool, EmbeddedNearTool, EmbeddedReadTelemetry, EmbeddedRelabelTool,
    EmbeddedRelateTool, EmbeddedSummariesAuditTool, EmbeddedTemporalMoveTool, EmbeddedTraceTool,
    EmbeddedVisualProjectionTool, EmbeddedWakeTool,
};
use super::judgement_reranker::JudgementReranker;
use super::judgement_source::load_judgement;
use super::lexical_bridge_file::load_lexical_bridge;
use super::loopback_semantic_retriever::LoopbackSemanticRetriever;
use super::wake_focus_judge::WakeFocusJudge;
use crate::contract::{TIME_TOOL, TimeMove};
use crate::serving::environment::{
    TYPESAFE_API_KEY_ENV, TYPESAFE_CASSETTE_ENV, TYPESAFE_CASSETTE_MODE_ENV, optional_env_string,
};
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::ports::semantic_candidate_provider::SemanticCandidateProvider;
use crate::serving::tool_success_result;
use crate::serving::{KernelMcpToolBackend, KernelMcpToolFuture, ToolError};
use kmp_domain::TemporalDirection;
use kmp_embedded::{CommitNativeBundle, EmbeddedKernel};
use kmp_proto_mapping::v1beta1::{LexicalBridge, LexicalIndexCache};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

/// Opt-in beside the store for relations proposed after each write.
const WRITE_RELATIONS_FILE: &str = "write-relations.json";

/// In-process kernel backend: the same JSON argument builders and response
/// shapes as live mode, with the application service called directly instead
/// of a gRPC channel — identical tool JSON by construction.
pub struct EmbeddedKernelMcpBackend {
    kernel: EmbeddedKernel,
    data_dir: String,
    commit_native: Option<CommitNativeBundle>,
    /// The word table `ask` bridges languages with, read once beside the
    /// store. Silent when none is installed.
    lexical_bridge: LexicalBridge,
    lexical_cache: Arc<LexicalIndexCache>,
    semantic: Result<Option<Arc<dyn SemanticCandidateProvider>>, String>,
    /// TypeSafe Jev for `kmp_curate`, opted into per store. Off without
    /// `typesafe.json`; an error names why a present opt-in cannot run.
    judgement: Result<Option<Arc<dyn JudgementModel>>, String>,
    /// Ask re-ranking by the same model, opted into separately because it
    /// sends text on every Ask.
    rerank: Result<Option<Arc<JudgementReranker>>, String>,
    /// Wake focused on its intent by the same model, its own opt-in.
    wake_focus: Result<Option<Arc<WakeFocusJudge>>, String>,
    /// Relations proposed after each write, opted into by
    /// `write-relations.json` beside `typesafe.json`.
    write_relations: bool,
    curate_reviews: CurateReviewCache,
    curate_doubts: CurateDoubtCache,
}

impl EmbeddedKernelMcpBackend {
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        Self::open_with_engine(
            data_dir,
            kmp_embedded::default_engine_for_data_dir(data_dir),
        )
    }

    /// `engine` is what a fresh directory gets; an existing one must agree
    /// (ADR-018). `None` defers to the directory, or the default.
    pub fn open_with_engine(
        data_dir: &Path,
        engine: Option<kmp_embedded::StorageEngine>,
    ) -> Result<Self, String> {
        Self::open_with_engine_and_commit_native(data_dir, engine, None)
    }

    pub fn open_with_engine_and_commit_native(
        data_dir: &Path,
        engine: Option<kmp_embedded::StorageEngine>,
        commit_native: Option<CommitNativeBundle>,
    ) -> Result<Self, String> {
        let kernel = EmbeddedKernel::open_with_engine(data_dir, engine)
            .map_err(|error| error.to_string())?;
        let judgement = load_judgement(
            data_dir,
            optional_env_string(TYPESAFE_API_KEY_ENV),
            optional_env_string(TYPESAFE_CASSETTE_ENV),
            optional_env_string(TYPESAFE_CASSETTE_MODE_ENV),
        );
        let rerank = JudgementReranker::load(data_dir, &judgement);
        let wake_focus = WakeFocusJudge::load(data_dir, &judgement);
        let write_relations = data_dir.join(WRITE_RELATIONS_FILE).is_file();
        Ok(Self {
            kernel,
            data_dir: data_dir.display().to_string(),
            commit_native,
            lexical_bridge: load_lexical_bridge(data_dir),
            lexical_cache: Arc::default(),
            semantic: LoopbackSemanticRetriever::load(data_dir),
            judgement,
            rerank,
            wake_focus,
            write_relations,
            curate_reviews: CurateReviewCache::default(),
            curate_doubts: CurateDoubtCache::default(),
        })
    }

    /// The table `ask` bridges languages with on this store.
    pub fn lexical_bridge(&self) -> &LexicalBridge {
        &self.lexical_bridge
    }

    /// The storage engine this session's store is on.
    pub fn engine(&self) -> kmp_embedded::StorageEngine {
        self.kernel.engine()
    }

    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }

    /// The opened kernel, for composition roots that mount additional
    /// in-process surfaces (the viewer) over this same session's store.
    pub fn kernel(&self) -> &EmbeddedKernel {
        &self.kernel
    }
}

impl KernelMcpToolBackend for EmbeddedKernelMcpBackend {
    fn backend_name(&self) -> &'static str {
        "embedded"
    }

    fn bridges_languages(&self) -> bool {
        !self.lexical_bridge.is_silent()
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        let service = self.kernel.service();
        let observer = self.kernel.quality_observer();
        Box::pin(async move {
            let telemetry = EmbeddedReadTelemetry::new(observer.as_ref());
            match name {
                "kmp_ingest" | "kernel_remember" | "kernel_ingest_context" => {
                    EmbeddedIngestTool::new(
                        &service,
                        self.commit_native.as_ref(),
                        self.kernel.store(),
                    )
                    .call(arguments)
                    .await
                }
                "kmp_wake" => {
                    EmbeddedWakeTool::new(&service, telemetry, &self.wake_focus)
                        .call(arguments)
                        .await
                }
                "kmp_ask" => {
                    EmbeddedAskTool::new(
                        &service,
                        telemetry,
                        &self.lexical_bridge,
                        &self.semantic,
                        &self.rerank,
                        &self.lexical_cache,
                    )
                    .call(arguments)
                    .await
                }
                TIME_TOOL => {
                    let (direction, name) = match TimeMove::from_arguments(arguments)? {
                        TimeMove::Near => {
                            return EmbeddedNearTool::new(&service).call(arguments).await;
                        }
                        TimeMove::Goto => (TemporalDirection::Goto, "goto"),
                        TimeMove::Rewind => (TemporalDirection::Rewind, "rewind"),
                        TimeMove::Forward => (TemporalDirection::Forward, "forward"),
                    };
                    EmbeddedTemporalMoveTool::new(&service, direction, name)
                        .call(arguments)
                        .await
                }
                "kmp_relate" => {
                    EmbeddedRelateTool::new(&service, telemetry, &self.lexical_bridge)
                        .call(arguments)
                        .await
                }
                "kmp_curate" => {
                    let (judgement, warning) = match &self.judgement {
                        Ok(Some(model)) => (Some(model.as_ref()), None),
                        Ok(None) => (None, None),
                        Err(error) => (None, Some(error.as_str())),
                    };
                    // Internal: the write dispatcher asks for the relations
                    // the memories it just wrote are missing. Off unless the
                    // store opted in and Jev can run.
                    let mut arguments = arguments.clone();
                    if arguments.get("mode").and_then(Value::as_str) == Some("write_proposals") {
                        if !self.write_relations || judgement.is_none() {
                            return Ok(tool_success_result(json!({"enabled": false})));
                        }
                        arguments["mode"] = json!("review");
                    }
                    let arguments = &arguments;
                    EmbeddedCurateTool::new(
                        &service,
                        telemetry,
                        &self.lexical_bridge,
                        judgement,
                        warning,
                        &self.curate_reviews,
                        &self.curate_doubts,
                    )
                    .call(arguments)
                    .await
                }
                "kmp_trace" => {
                    EmbeddedTraceTool::new(&service, telemetry)
                        .call(arguments)
                        .await
                }
                "kmp_inspect" => EmbeddedInspectTool::new(&service).call(arguments).await,
                "kmp_relabel" => EmbeddedRelabelTool::new(&service).call(arguments).await,
                "kmp_condense" => {
                    EmbeddedCondenseTool::new(
                        &service,
                        self.commit_native.as_ref(),
                        self.kernel.store(),
                    )
                    .call(arguments)
                    .await
                }
                // The one read that is made off the event log rather than
                // the projections: a summary's earlier revisions are what
                // say whether the text moved after it was written.
                "kmp_summaries_audit" => {
                    EmbeddedSummariesAuditTool::new(self.kernel.store())
                        .call(arguments)
                        .await
                }
                "kmp_view_read_nodes" => {
                    super::embedded::EmbeddedMemoryNodesTool::new(&service)
                        .call(arguments)
                        .await
                }
                "kmp_view_read_projection" => {
                    EmbeddedVisualProjectionTool::new(&service)
                        .call(arguments)
                        .await
                }
                other => Err(ToolError::unknown_tool(format!(
                    "unknown KMP tool `{other}`"
                ))),
            }
        })
    }
}
