use std::path::Path;

use kmp_domain::{PortError, QualityMetricsObserver, QualityObservationContext, TemporalDirection};
use kmp_embedded::{CommitNativeBundle, EmbeddedKernel, EmbeddedMemoryService};
use kmp_proto_mapping::v1beta1::recall_projection::{project_ask_response, project_wake_response};
use kmp_proto_mapping::v1beta1::{
    LexicalBridge, ask_query_from_proto, ask_response_from_result, ingest_command_from_proto,
    ingest_response_from_outcome, inspect_query_from_proto, inspect_response_from_result,
    temporal_query_from_move_proto, temporal_query_from_near_proto, temporal_response_from_result,
    trace_query_from_proto, trace_response_from_result, visual_projection_query_from_proto,
    visual_projection_response_from_result, wake_query_from_proto, wake_response_from_result,
};
use serde_json::Value;

use crate::projection::{
    ask_from_response, dry_run_ingest_from_plan, enforce_inspect_output_budget,
    enforce_temporal_output_budget, ingest_from_response, inspect_from_response,
    temporal_from_response, trace_from_response, visual_projection_from_response,
    wake_from_response,
};
use crate::serving::adapters::grpc::requests::{
    ask_request_from_arguments, ingest_request_from_arguments, inspect_request_from_arguments,
    temporal_move_request_from_arguments, temporal_near_request_from_arguments,
    trace_request_from_arguments, visual_projection_request_from_arguments,
    wake_request_from_arguments,
};
use crate::serving::{KernelMcpToolBackend, KernelMcpToolFuture};
use crate::serving::{ToolError, ToolErrorCode};
use crate::serving::{app_data_success_result, tool_success_result};
use crate::write::build_ingest_plan;

use super::embedded_errors::{kernel_error, mapping_error, temporal_error};
use super::lexical_bridge_file::load_lexical_bridge;

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
        let quality_observer = self.kernel.quality_observer();
        let commit_native = self.commit_native.as_ref();
        let store = self.kernel.store();
        let bridge = &self.lexical_bridge;
        Box::pin(async move {
            let writes = matches!(
                name,
                "kmp_ingest" | "kernel_remember" | "kernel_ingest_context"
            ) && !arguments
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let Some(commit_native) = commit_native.filter(|_| writes) else {
                return embedded_tool_result(
                    &service,
                    quality_observer.as_ref(),
                    bridge,
                    name,
                    arguments,
                )
                .await;
            };
            let pending = commit_native
                .begin_write(store)
                .await
                .map_err(commit_native_preflight_error)?;
            let result =
                embedded_tool_result(&service, quality_observer.as_ref(), bridge, name, arguments)
                    .await;
            match result {
                Ok(result) => {
                    let header = commit_native.publish(store, &pending).await.map_err(|error| {
                        ToolError::backend(format!(
                            "memory write committed, but the commit-native bundle `{}` did not: \
                             {error}. The pending marker remains; run `kmp-mcp export` before \
                             trusting or committing this memory.",
                            commit_native.path().display()
                        ))
                    })?;
                    pending.complete().map_err(|error| {
                        ToolError::backend(format!(
                            "memory and bundle {} are current at snapshot {}, but the pending \
                             marker could not be cleared: {error}",
                            commit_native.path().display(),
                            header.snapshot_id
                        ))
                    })?;
                    Ok(result)
                }
                Err(error)
                    if matches!(
                        error.code,
                        ToolErrorCode::InvalidArgument
                            | ToolErrorCode::NotFound
                            | ToolErrorCode::Conflict
                            | ToolErrorCode::UnknownTool
                    ) =>
                {
                    pending.complete().map_err(|marker_error| {
                        ToolError::backend(format!(
                            "write was rejected as {}, but its commit-native pending marker \
                             could not be cleared: {marker_error}",
                            error.code
                        ))
                    })?;
                    Err(error)
                }
                Err(error) => Err(error),
            }
        })
    }
}

fn commit_native_preflight_error(error: PortError) -> ToolError {
    let message = format!(
        "memory write was refused before changing the store because its committed bundle could \
         not be guarded: {error}"
    );
    match error {
        PortError::Conflict(_) => ToolError::conflict(message),
        PortError::Unavailable(_) => ToolError::unavailable(message),
        PortError::InvalidState(_) => ToolError::backend(message),
    }
}

fn observe_quality(
    observer: &dyn QualityMetricsObserver,
    rpc: &str,
    root_node_id: &str,
    role: &str,
    revision: u64,
    quality: &kmp_domain::BundleQualityMetrics,
) {
    observer.observe(
        quality,
        &QualityObservationContext {
            rpc: rpc.to_string(),
            root_node_id: root_node_id.to_string(),
            role: role.to_string(),
            revision: Some(revision),
        },
    );
}

async fn embedded_tool_result(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    bridge: &LexicalBridge,
    name: &str,
    arguments: &Value,
) -> Result<Value, ToolError> {
    match name {
        "kmp_ingest" | "kernel_remember" | "kernel_ingest_context" => {
            embedded_ingest(service, arguments).await
        }
        "kmp_wake" => embedded_wake(service, observer, arguments).await,
        "kmp_ask" => embedded_ask(service, observer, bridge, arguments).await,
        "kmp_goto" => {
            embedded_temporal(
                service,
                observer,
                TemporalDirection::Goto,
                "goto",
                arguments,
            )
            .await
        }
        "kmp_near" => embedded_near(service, observer, arguments).await,
        "kmp_rewind" => {
            embedded_temporal(
                service,
                observer,
                TemporalDirection::Rewind,
                "rewind",
                arguments,
            )
            .await
        }
        "kmp_forward" => {
            embedded_temporal(
                service,
                observer,
                TemporalDirection::Forward,
                "forward",
                arguments,
            )
            .await
        }
        "kmp_trace" => embedded_trace(service, observer, arguments).await,
        "kmp_inspect" => embedded_inspect(service, arguments).await,
        "kmp_view_read_projection" => embedded_visual_projection(service, arguments).await,
        other => Err(ToolError::unknown_tool(format!(
            "unknown KMP tool `{other}`"
        ))),
    }
}

async fn embedded_visual_projection(
    service: &EmbeddedMemoryService,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        visual_projection_request_from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let about = request.about.clone();
    let query =
        visual_projection_query_from_proto(request).map_err(|status| mapping_error(&status))?;
    let result = service
        .visual_projection(query)
        .await
        .map_err(kernel_error("project_visual", &about))?;
    Ok(app_data_success_result(visual_projection_from_response(
        visual_projection_response_from_result(result),
    )))
}

async fn embedded_ingest(
    service: &EmbeddedMemoryService,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = ingest_request_from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    if request.dry_run {
        let plan = build_ingest_plan(arguments)?;
        return Ok(tool_success_result(dry_run_ingest_from_plan(&plan)));
    }
    let command = ingest_command_from_proto(request).map_err(|status| mapping_error(&status))?;
    let about = command.about.clone();
    let outcome = service
        .ingest(command)
        .await
        .map_err(kernel_error("ingest", &about))?;
    Ok(tool_success_result(ingest_from_response(
        ingest_response_from_outcome(outcome),
    )))
}

async fn embedded_wake(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = wake_request_from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let query = wake_query_from_proto(request.clone()).map_err(|status| mapping_error(&status))?;
    let intent = query.intent.clone();
    let max_entries = query.max_entries;
    let temporal = query.temporal.clone();
    let about = query.about.clone();
    let result = service
        .wake(query)
        .await
        .map_err(kernel_error("wake", &about))?;
    observe_quality(
        observer,
        "kmp_wake",
        result.bundle.root_node_id().as_str(),
        result.bundle.role().as_str(),
        result.bundle.metadata().revision,
        &result.rendered.quality,
    );
    let response = project_wake_response(
        wake_response_from_result(&intent, max_entries, result, &temporal)
            .map_err(|status| mapping_error(&status))?,
        &request,
    )
    .map_err(|error| ToolError::invalid_argument(error.to_string()))?;
    Ok(tool_success_result(wake_from_response(response)))
}

async fn embedded_ask(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    bridge: &LexicalBridge,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = ask_request_from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let query = ask_query_from_proto(request.clone()).map_err(|status| mapping_error(&status))?;
    let question = query.question.clone();
    let asked_as = query.asked_as.clone();
    let policy = query.answer_policy;
    let max_entries = query.max_entries;
    let temporal = query.temporal.clone();
    let about = query.about.clone();
    let result = service
        .ask(query)
        .await
        .map_err(kernel_error("ask", &about))?;
    observe_quality(
        observer,
        "kmp_ask",
        result.bundle.root_node_id().as_str(),
        result.bundle.role().as_str(),
        result.bundle.metadata().revision,
        &result.rendered.quality,
    );
    let response = project_ask_response(
        ask_response_from_result(
            &question,
            asked_as.as_deref(),
            policy,
            max_entries,
            result,
            bridge,
            &temporal,
        )
        .map_err(|status| mapping_error(&status))?,
        &request,
    )
    .map_err(|error| ToolError::invalid_argument(error.to_string()))?;
    Ok(tool_success_result(ask_from_response(response)))
}

async fn embedded_temporal(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    direction: TemporalDirection,
    direction_name: &str,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = temporal_move_request_from_arguments(arguments, direction_name)
        .map_err(ToolError::invalid_argument)?;
    let requested_cursor = request.cursor.clone().unwrap_or_default();
    let query = temporal_query_from_move_proto(request, direction)
        .map_err(|status| mapping_error(&status))?;
    let about = query.about.clone();
    let result = service
        .temporal(query)
        .await
        .map_err(temporal_error(direction_name, &about))?;
    observe_quality(
        observer,
        &format!("kmp_{direction_name}"),
        result.source_bundle.root_node_id().as_str(),
        result.source_bundle.role().as_str(),
        result.source_bundle.metadata().revision,
        &result.quality,
    );
    Ok(tool_success_result(enforce_temporal_output_budget(
        temporal_from_response(temporal_response_from_result(
            requested_cursor,
            direction,
            result,
        )),
        arguments,
    )?))
}

async fn embedded_near(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        temporal_near_request_from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let requested_cursor = request.around.clone().unwrap_or_default();
    let query = temporal_query_from_near_proto(request).map_err(|status| mapping_error(&status))?;
    let about = query.about.clone();
    let result = service
        .temporal(query)
        .await
        .map_err(temporal_error("near", &about))?;
    observe_quality(
        observer,
        "kmp_near",
        result.source_bundle.root_node_id().as_str(),
        result.source_bundle.role().as_str(),
        result.source_bundle.metadata().revision,
        &result.quality,
    );
    Ok(tool_success_result(enforce_temporal_output_budget(
        temporal_from_response(temporal_response_from_result(
            requested_cursor,
            TemporalDirection::Near,
            result,
        )),
        arguments,
    )?))
}

async fn embedded_trace(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = trace_request_from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let query = trace_query_from_proto(request).map_err(|status| mapping_error(&status))?;
    let page = query.page.clone();
    let from = query.from.clone();
    let result = service
        .trace(query)
        .await
        .map_err(kernel_error("trace", &from))?;
    observe_quality(
        observer,
        "kmp_trace",
        result.path_bundle.root_node_id().as_str(),
        result.path_bundle.role().as_str(),
        result.path_bundle.metadata().revision,
        &result.rendered.quality,
    );
    Ok(tool_success_result(trace_from_response(
        trace_response_from_result(result, page),
    )))
}

async fn embedded_inspect(
    service: &EmbeddedMemoryService,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = inspect_request_from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let query = inspect_query_from_proto(request).map_err(|status| mapping_error(&status))?;
    let ref_id = query.ref_id.clone();
    let result = service
        .inspect(query)
        .await
        .map_err(kernel_error("inspect", &ref_id))?;
    Ok(tool_success_result(enforce_inspect_output_budget(
        inspect_from_response(inspect_response_from_result(result)),
        arguments,
    )?))
}
