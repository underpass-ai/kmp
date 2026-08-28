use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kmp_domain::{QualityMetricsObserver, QualityObservationContext, TemporalDirection};
use kmp_embedded::{CommitNativeBundle, EmbeddedKernel, EmbeddedMemoryService};
use kmp_proto_mapping::v1beta1::recall_projection::{project_ask_response, project_wake_response};
use kmp_proto_mapping::v1beta1::{
    ask_query_from_proto, ask_response_from_result, ingest_command_from_proto,
    ingest_response_from_outcome, inspect_query_from_proto, inspect_response_from_result,
    temporal_query_from_move_proto, temporal_query_from_near_proto, temporal_response_from_result,
    trace_query_from_proto, trace_response_from_result, visual_projection_query_from_proto,
    visual_projection_response_from_result, wake_query_from_proto, wake_response_from_result,
};
use serde_json::Value;

use crate::backend::{KernelMcpToolBackend, KernelMcpToolFuture};
use crate::grpc::requests::{
    ask_request_from_arguments, ingest_request_from_arguments, inspect_request_from_arguments,
    temporal_move_request_from_arguments, temporal_near_request_from_arguments,
    trace_request_from_arguments, visual_projection_request_from_arguments,
    wake_request_from_arguments,
};
use crate::ingest::build_ingest_plan;
use crate::kmp::{
    ask_from_response, dry_run_ingest_from_plan, enforce_inspect_output_budget,
    enforce_temporal_output_budget, ingest_from_response, inspect_from_response,
    temporal_from_response, trace_from_response, visual_projection_from_response,
    wake_from_response,
};
use crate::protocol::{app_data_success_result, tool_success_result};
use crate::tool_error::{ToolError, ToolErrorCode};

/// In-process kernel backend: the same JSON argument builders and response
/// shapes as live mode, with the application service called directly instead
/// of a gRPC channel — identical tool JSON by construction.
pub struct EmbeddedKernelMcpBackend {
    kernel: EmbeddedKernel,
    data_dir: String,
    commit_native: Option<CommitNativeBundle>,
}

/// Embedded backend that opens the store on the first memory call and retries
/// transient redb ownership conflicts on later calls.
///
/// MCP discovery (`initialize` and `tools/list`) does not need the database.
/// Keeping that surface alive means a host does not permanently lose KMP just
/// because another editor owned a redb store during process startup. Once the
/// owner exits, the next tool call opens the store and the same MCP process
/// recovers without a host restart.
pub struct RetryingEmbeddedKernelMcpBackend {
    data_dir: PathBuf,
    engine: Option<kmp_embedded::StorageEngine>,
    commit_native: Option<CommitNativeBundle>,
    opened: Mutex<Option<Arc<EmbeddedKernelMcpBackend>>>,
}

impl RetryingEmbeddedKernelMcpBackend {
    pub fn new(data_dir: &Path, engine: Option<kmp_embedded::StorageEngine>) -> Self {
        Self::new_with_commit_native(data_dir, engine, None)
    }

    pub fn new_with_commit_native(
        data_dir: &Path,
        engine: Option<kmp_embedded::StorageEngine>,
        commit_native: Option<CommitNativeBundle>,
    ) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            engine,
            commit_native,
            opened: Mutex::new(None),
        }
    }

    /// Best engine label available without opening or stamping the store.
    pub fn declared_engine(&self) -> Option<kmp_embedded::StorageEngine> {
        let stamp = std::fs::read_to_string(self.data_dir.join("FORMAT_VERSION"))
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .and_then(|version| match version {
                1 => Some(kmp_embedded::StorageEngine::Redb),
                2 => Some(kmp_embedded::StorageEngine::Sqlite),
                _ => None,
            });
        stamp.or(self.engine)
    }

    fn opened_backend(&self) -> Result<Arc<EmbeddedKernelMcpBackend>, String> {
        if let Some(backend) = self
            .opened
            .lock()
            .map_err(|_| "embedded backend state lock is poisoned".to_string())?
            .as_ref()
            .cloned()
        {
            return Ok(backend);
        }

        let mut last_error = String::new();
        for attempt in 0..3 {
            match EmbeddedKernelMcpBackend::open_with_engine_and_commit_native(
                &self.data_dir,
                self.engine,
                self.commit_native.clone(),
            ) {
                Ok(backend) => {
                    let backend = Arc::new(backend);
                    let mut opened = self
                        .opened
                        .lock()
                        .map_err(|_| "embedded backend state lock is poisoned".to_string())?;
                    let winner = opened.get_or_insert_with(|| Arc::clone(&backend));
                    return Ok(Arc::clone(winner));
                }
                Err(error) => {
                    let transient_lock = error.contains("Cannot acquire lock");
                    last_error = error;
                    if !transient_lock || attempt == 2 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100 * (attempt + 1) as u64));
                }
            }
        }
        Err(format!(
            "embedded store is temporarily unavailable; the MCP server is still running and the next tool call will retry: {last_error}"
        ))
    }
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
        })
    }

    /// The storage engine this session's store is on.
    pub fn engine(&self) -> kmp_embedded::StorageEngine {
        self.kernel.engine()
    }

    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }

    /// The opened kernel, for composition roots that mount additional
    /// in-process surfaces (the viewer) over this same session's store — on
    /// the redb engine the only way to observe it live under the ADR-011
    /// single-writer lock.
    pub fn kernel(&self) -> &EmbeddedKernel {
        &self.kernel
    }
}

impl KernelMcpToolBackend for EmbeddedKernelMcpBackend {
    fn backend_name(&self) -> &'static str {
        "embedded"
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        let service = self.kernel.service();
        let quality_observer = self.kernel.quality_observer();
        let commit_native = self.commit_native.as_ref();
        let store = self.kernel.store();
        Box::pin(async move {
            let writes = matches!(
                name,
                "kmp_ingest" | "kernel_remember" | "kernel_ingest_context"
            ) && !arguments
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let Some(commit_native) = commit_native.filter(|_| writes) else {
                return embedded_tool_result(&service, quality_observer.as_ref(), name, arguments)
                    .await;
            };
            let pending = commit_native.begin_write().map_err(|error| {
                ToolError::unavailable(format!(
                    "memory write was refused before changing the store because its committed \
                     bundle could not be guarded: {error}"
                ))
            })?;
            let result =
                embedded_tool_result(&service, quality_observer.as_ref(), name, arguments).await;
            match result {
                Ok(result) => {
                    let header = commit_native.publish(store).await.map_err(|error| {
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

impl KernelMcpToolBackend for RetryingEmbeddedKernelMcpBackend {
    fn backend_name(&self) -> &'static str {
        "embedded"
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        Box::pin(async move {
            let backend = self.opened_backend()?;
            backend.call_tool(name, arguments).await
        })
    }
}

fn mapping_error(status: &tonic::Status) -> ToolError {
    ToolError::invalid_argument(status.message())
}

/// Carries the kernel's own classification out to the caller.
///
/// The kernel already knows what went wrong — `ApplicationError` and
/// `PortError` are typed — and that knowledge used to be thrown away at this
/// boundary and reconstructed by matching English words further downstream.
/// Nothing here reads the message.
fn kernel_error<'a>(
    operation: &'a str,
    about: &'a str,
) -> impl FnOnce(kmp_application::ApplicationError) -> ToolError + 'a {
    use kmp_application::ApplicationError;
    use kmp_domain::{DomainError, PortError};

    move |error| {
        let code = match &error {
            ApplicationError::RetryableConflict(reason) => {
                return ToolError::conflict(format!(
                    "embedded kernel {operation} write conflict for `{about}`: the store moved \
                     while this write was being prepared, so this attempt was not applied. It \
                     is safe to retry the same logical write with the same `idempotency_key`; \
                     if an earlier attempt landed, idempotency returns that success instead of \
                     duplicating memory. Kernel detail: {reason}"
                ));
            }
            ApplicationError::NotFound(_) => ToolErrorCode::NotFound,
            ApplicationError::Validation(_) => ToolErrorCode::InvalidArgument,
            // A domain error is an invariant the payload broke, so the caller
            // can fix it. `EmptyValue` naming a field is the clearest case.
            ApplicationError::Domain(DomainError::EmptyValue(_)) => ToolErrorCode::InvalidArgument,
            // `InvalidState` is the ambiguous one: it covers both a payload
            // the model rejects and a store that cannot serve the request.
            // It stays a backend error, because telling an agent to fix its
            // arguments when nothing about them is wrong is the failure this
            // whole change exists to remove.
            ApplicationError::Domain(DomainError::InvalidState(_)) => ToolErrorCode::BackendError,
            ApplicationError::Ports(PortError::Conflict(_)) => ToolErrorCode::Conflict,
            ApplicationError::Ports(PortError::Unavailable(_)) => ToolErrorCode::Unavailable,
            ApplicationError::Ports(PortError::InvalidState(_)) => ToolErrorCode::BackendError,
        };
        let outcome = if code == ToolErrorCode::Conflict {
            "conflict"
        } else {
            "failed"
        };
        ToolError::new(
            code,
            format!("embedded kernel {operation} {outcome} for `{about}`: {error}"),
        )
    }
}

/// Temporal selection turns a domain `InvalidState` into a caller error: the
/// temporal domain uses that variant for an unresolved/invalid cursor, and the
/// gRPC service exposes the same condition as `INVALID_ARGUMENT`. This is
/// operation-specific classification, not message matching; port/store
/// `InvalidState` remains a backend failure through `kernel_error`.
fn temporal_error<'a>(
    operation: &'a str,
    about: &'a str,
) -> impl FnOnce(kmp_application::ApplicationError) -> ToolError + 'a {
    move |error| {
        if matches!(
            error,
            kmp_application::ApplicationError::Domain(kmp_domain::DomainError::InvalidState(_))
        ) {
            return ToolError::invalid_argument(format!(
                "embedded kernel {operation} failed for `{about}`: {error}"
            ));
        }
        kernel_error(operation, about)(error)
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
    name: &str,
    arguments: &Value,
) -> Result<Value, ToolError> {
    match name {
        "kmp_ingest" | "kernel_remember" | "kernel_ingest_context" => {
            embedded_ingest(service, arguments).await
        }
        "kmp_wake" => embedded_wake(service, observer, arguments).await,
        "kmp_ask" => embedded_ask(service, observer, arguments).await,
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
        wake_response_from_result(&intent, max_entries, result),
        &request,
    )
    .map_err(|error| ToolError::invalid_argument(error.to_string()))?;
    Ok(tool_success_result(wake_from_response(response)))
}

async fn embedded_ask(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = ask_request_from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let query = ask_query_from_proto(request.clone()).map_err(|status| mapping_error(&status))?;
    let question = query.question.clone();
    let policy = query.answer_policy;
    let max_entries = query.max_entries;
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
        ask_response_from_result(&question, policy, max_entries, result),
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

#[cfg(test)]
mod retry_tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn optimistic_write_conflicts_name_the_safe_retry_contract() {
        let error = kernel_error("ingest", "incident:pool-saturation")(
            kmp_application::ApplicationError::RetryableConflict(
                "expected revision 16, current is 17".to_string(),
            ),
        );

        assert_eq!(error.code, ToolErrorCode::Conflict);
        assert!(error.message.contains("write conflict"), "{error}");
        assert!(error.message.contains("attempt was not applied"), "{error}");
        assert!(error.message.contains("safe to retry"), "{error}");
        assert!(error.message.contains("same `idempotency_key`"), "{error}");
        assert!(error.message.contains("expected revision 16"), "{error}");
    }

    #[tokio::test]
    async fn a_redb_startup_lock_does_not_permanently_disable_the_backend() {
        let data_dir = tempfile::tempdir().expect("temp data dir");
        std::fs::write(data_dir.path().join("FORMAT_VERSION"), "1\n").expect("legacy format stamp");
        let holder = EmbeddedKernel::open_with_engine(
            data_dir.path(),
            Some(kmp_embedded::StorageEngine::Redb),
        )
        .expect("first redb owner");
        let backend = RetryingEmbeddedKernelMcpBackend::new(data_dir.path(), None);
        let request = json!({"ref": "missing:test"});

        let locked = backend
            .call_tool("kmp_inspect", &request)
            .await
            .expect_err("the other owner still holds redb");
        assert!(
            locked.message.contains("server is still running"),
            "{locked}"
        );

        drop(holder);
        let recovered = backend
            .call_tool("kmp_inspect", &request)
            .await
            .expect_err("the node is absent, but the store now opens");
        assert!(
            recovered.message.to_ascii_lowercase().contains("not found"),
            "{recovered}"
        );
        assert!(
            !recovered.message.contains("temporarily unavailable"),
            "{recovered}"
        );
    }
}
