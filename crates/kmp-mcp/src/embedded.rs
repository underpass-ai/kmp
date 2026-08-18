use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kmp_domain::{QualityMetricsObserver, QualityObservationContext, TemporalDirection};
use kmp_embedded::{EmbeddedKernel, EmbeddedMemoryService};
use kmp_proto_mapping::v1beta1::{
    ask_query_from_proto, ask_response_from_result, ingest_command_from_proto,
    ingest_response_from_outcome, inspect_query_from_proto, inspect_response_from_result,
    temporal_query_from_move_proto, temporal_query_from_near_proto, temporal_response_from_result,
    trace_query_from_proto, trace_response_from_result, wake_query_from_proto,
    wake_response_from_result,
};
use serde_json::Value;

use crate::backend::{KernelMcpToolBackend, KernelMcpToolFuture};
use crate::grpc::requests::{
    ask_request_from_arguments, ingest_request_from_arguments, inspect_request_from_arguments,
    temporal_move_request_from_arguments, temporal_near_request_from_arguments,
    trace_request_from_arguments, wake_request_from_arguments,
};
use crate::ingest::build_ingest_plan;
use crate::kmp::{
    ask_from_response, dry_run_ingest_from_plan, ingest_from_response, inspect_from_response,
    temporal_from_response, trace_from_response, try_enforce_recall_output_budget,
    wake_from_response,
};
use crate::protocol::tool_success_result;

/// In-process kernel backend: the same JSON argument builders and response
/// shapes as live mode, with the application service called directly instead
/// of a gRPC channel — identical tool JSON by construction.
pub struct EmbeddedKernelMcpBackend {
    kernel: EmbeddedKernel,
    data_dir: String,
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
    opened: Mutex<Option<Arc<EmbeddedKernelMcpBackend>>>,
}

impl RetryingEmbeddedKernelMcpBackend {
    pub fn new(data_dir: &Path, engine: Option<kmp_embedded::StorageEngine>) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            engine,
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
            match EmbeddedKernelMcpBackend::open_with_engine(&self.data_dir, self.engine) {
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
        let kernel = EmbeddedKernel::open_with_engine(data_dir, engine)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            kernel,
            data_dir: data_dir.display().to_string(),
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
        Box::pin(async move {
            embedded_tool_result(&service, quality_observer.as_ref(), name, arguments).await
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

fn mapping_error(status: &tonic::Status) -> String {
    status.message().to_string()
}

fn kernel_error<'a>(
    operation: &'a str,
    about: &'a str,
) -> impl FnOnce(kmp_application::ApplicationError) -> String + 'a {
    move |error| format!("embedded kernel {operation} failed for `{about}`: {error}")
}

fn observe_quality(
    observer: &dyn QualityMetricsObserver,
    rpc: &str,
    root_node_id: &str,
    role: &str,
    quality: &kmp_domain::BundleQualityMetrics,
) {
    observer.observe(
        quality,
        &QualityObservationContext {
            rpc: rpc.to_string(),
            root_node_id: root_node_id.to_string(),
            role: role.to_string(),
        },
    );
}

async fn embedded_tool_result(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    match name {
        "kernel_ingest" | "kernel_remember" | "kernel_ingest_context" => {
            embedded_ingest(service, arguments).await
        }
        "kernel_wake" => embedded_wake(service, observer, arguments).await,
        "kernel_ask" => embedded_ask(service, observer, arguments).await,
        "kernel_goto" => {
            embedded_temporal(
                service,
                observer,
                TemporalDirection::Goto,
                "goto",
                arguments,
            )
            .await
        }
        "kernel_near" => embedded_near(service, observer, arguments).await,
        "kernel_rewind" => {
            embedded_temporal(
                service,
                observer,
                TemporalDirection::Rewind,
                "rewind",
                arguments,
            )
            .await
        }
        "kernel_forward" => {
            embedded_temporal(
                service,
                observer,
                TemporalDirection::Forward,
                "forward",
                arguments,
            )
            .await
        }
        "kernel_trace" => embedded_trace(service, observer, arguments).await,
        "kernel_inspect" => embedded_inspect(service, arguments).await,
        other => Err(format!("unknown KMP tool `{other}`")),
    }
}

async fn embedded_ingest(
    service: &EmbeddedMemoryService,
    arguments: &Value,
) -> Result<Value, String> {
    let request = ingest_request_from_arguments(arguments)?;
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
) -> Result<Value, String> {
    let request = wake_request_from_arguments(arguments)?;
    let query = wake_query_from_proto(request).map_err(|status| mapping_error(&status))?;
    let intent = query.intent.clone();
    let max_entries = query.max_entries;
    let about = query.about.clone();
    let result = service
        .wake(query)
        .await
        .map_err(kernel_error("wake", &about))?;
    observe_quality(
        observer,
        "kernel_wake",
        result.bundle.root_node_id().as_str(),
        result.bundle.role().as_str(),
        &result.rendered.quality,
    );
    let structured = wake_from_response(wake_response_from_result(&intent, max_entries, result));
    Ok(tool_success_result(try_enforce_recall_output_budget(
        structured, arguments, 1600,
    )?))
}

async fn embedded_ask(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    arguments: &Value,
) -> Result<Value, String> {
    let request = ask_request_from_arguments(arguments)?;
    let query = ask_query_from_proto(request).map_err(|status| mapping_error(&status))?;
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
        "kernel_ask",
        result.bundle.root_node_id().as_str(),
        result.bundle.role().as_str(),
        &result.rendered.quality,
    );
    let structured = ask_from_response(ask_response_from_result(
        &question,
        policy,
        max_entries,
        result,
    ));
    Ok(tool_success_result(try_enforce_recall_output_budget(
        structured, arguments, 2400,
    )?))
}

async fn embedded_temporal(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    direction: TemporalDirection,
    direction_name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    let request = temporal_move_request_from_arguments(arguments, direction_name)?;
    let requested_cursor = request.cursor.clone().unwrap_or_default();
    let query = temporal_query_from_move_proto(request, direction)
        .map_err(|status| mapping_error(&status))?;
    let about = query.about.clone();
    let result = service
        .temporal(query)
        .await
        .map_err(kernel_error(direction_name, &about))?;
    observe_quality(
        observer,
        &format!("kernel_{direction_name}"),
        result.source_bundle.root_node_id().as_str(),
        result.source_bundle.role().as_str(),
        &result.quality,
    );
    Ok(tool_success_result(temporal_from_response(
        temporal_response_from_result(requested_cursor, direction, result),
    )))
}

async fn embedded_near(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    arguments: &Value,
) -> Result<Value, String> {
    let request = temporal_near_request_from_arguments(arguments)?;
    let requested_cursor = request.around.clone().unwrap_or_default();
    let query = temporal_query_from_near_proto(request).map_err(|status| mapping_error(&status))?;
    let about = query.about.clone();
    let result = service
        .temporal(query)
        .await
        .map_err(kernel_error("near", &about))?;
    observe_quality(
        observer,
        "kernel_near",
        result.source_bundle.root_node_id().as_str(),
        result.source_bundle.role().as_str(),
        &result.quality,
    );
    Ok(tool_success_result(temporal_from_response(
        temporal_response_from_result(requested_cursor, TemporalDirection::Near, result),
    )))
}

async fn embedded_trace(
    service: &EmbeddedMemoryService,
    observer: &dyn QualityMetricsObserver,
    arguments: &Value,
) -> Result<Value, String> {
    let request = trace_request_from_arguments(arguments)?;
    let query = trace_query_from_proto(request).map_err(|status| mapping_error(&status))?;
    let page = query.page.clone();
    let from = query.from.clone();
    let result = service
        .trace(query)
        .await
        .map_err(kernel_error("trace", &from))?;
    observe_quality(
        observer,
        "kernel_trace",
        result.path_bundle.root_node_id().as_str(),
        result.path_bundle.role().as_str(),
        &result.rendered.quality,
    );
    Ok(tool_success_result(trace_from_response(
        trace_response_from_result(result, page),
    )))
}

async fn embedded_inspect(
    service: &EmbeddedMemoryService,
    arguments: &Value,
) -> Result<Value, String> {
    let request = inspect_request_from_arguments(arguments)?;
    let query = inspect_query_from_proto(request).map_err(|status| mapping_error(&status))?;
    let ref_id = query.ref_id.clone();
    let result = service
        .inspect(query)
        .await
        .map_err(kernel_error("inspect", &ref_id))?;
    Ok(tool_success_result(inspect_from_response(
        inspect_response_from_result(result),
    )))
}

#[cfg(test)]
mod retry_tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn a_redb_startup_lock_does_not_permanently_disable_the_backend() {
        let data_dir = tempfile::tempdir().expect("temp data dir");
        let holder = EmbeddedKernel::open_with_engine(
            data_dir.path(),
            Some(kmp_embedded::StorageEngine::Redb),
        )
        .expect("first redb owner");
        let backend = RetryingEmbeddedKernelMcpBackend::new(data_dir.path(), None);
        let request = json!({"ref": "missing:test"});

        let locked = backend
            .call_tool("kernel_inspect", &request)
            .await
            .expect_err("the other owner still holds redb");
        assert!(locked.contains("server is still running"), "{locked}");

        drop(holder);
        let recovered = backend
            .call_tool("kernel_inspect", &request)
            .await
            .expect_err("the node is absent, but the store now opens");
        assert!(
            recovered.to_ascii_lowercase().contains("not found"),
            "{recovered}"
        );
        assert!(
            !recovered.contains("temporarily unavailable"),
            "{recovered}"
        );
    }
}
