use super::embedded::{
    EmbeddedAskTool, EmbeddedCondenseTool, EmbeddedIngestTool, EmbeddedInspectTool,
    EmbeddedNearTool, EmbeddedReadTelemetry, EmbeddedRelabelTool, EmbeddedRelateTool,
    EmbeddedTemporalMoveTool, EmbeddedTraceTool, EmbeddedVisualProjectionTool, EmbeddedWakeTool,
};
use super::lexical_bridge_file::load_lexical_bridge;
use super::loopback_semantic_retriever::LoopbackSemanticRetriever;
use crate::serving::ports::semantic_candidate_provider::SemanticCandidateProvider;
use crate::serving::{KernelMcpToolBackend, KernelMcpToolFuture, ToolError};
use kmp_domain::TemporalDirection;
use kmp_embedded::{CommitNativeBundle, EmbeddedKernel};
use kmp_proto_mapping::v1beta1::LexicalBridge;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

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
    semantic: Result<Option<Arc<dyn SemanticCandidateProvider>>, String>,
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
        Ok(Self {
            kernel,
            data_dir: data_dir.display().to_string(),
            commit_native,
            lexical_bridge: load_lexical_bridge(data_dir),
            semantic: LoopbackSemanticRetriever::load(data_dir),
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
                    EmbeddedWakeTool::new(&service, telemetry)
                        .call(arguments)
                        .await
                }
                "kmp_ask" => {
                    EmbeddedAskTool::new(&service, telemetry, &self.lexical_bridge, &self.semantic)
                        .call(arguments)
                        .await
                }
                "kmp_goto" => {
                    EmbeddedTemporalMoveTool::new(&service, TemporalDirection::Goto, "goto")
                        .call(arguments)
                        .await
                }
                "kmp_near" => EmbeddedNearTool::new(&service).call(arguments).await,
                "kmp_rewind" => {
                    EmbeddedTemporalMoveTool::new(&service, TemporalDirection::Rewind, "rewind")
                        .call(arguments)
                        .await
                }
                "kmp_forward" => {
                    EmbeddedTemporalMoveTool::new(&service, TemporalDirection::Forward, "forward")
                        .call(arguments)
                        .await
                }
                "kmp_relate" => {
                    EmbeddedRelateTool::new(&service, telemetry, &self.lexical_bridge)
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
                "kmp_condense" => EmbeddedCondenseTool::new(&service).call(arguments).await,
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
