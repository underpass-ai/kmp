use std::path::Path;

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
    temporal_from_response, trace_from_response, wake_from_response,
};
use crate::protocol::tool_success_result;

/// In-process kernel backend: the same JSON argument builders and response
/// shapes as live mode, with the application service called directly instead
/// of a gRPC channel — identical tool JSON by construction.
pub struct EmbeddedKernelMcpBackend {
    kernel: EmbeddedKernel,
    data_dir: String,
}

impl EmbeddedKernelMcpBackend {
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        Self::open_with_engine(data_dir, None)
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
    Ok(tool_success_result(wake_from_response(
        wake_response_from_result(&intent, max_entries, result),
    )))
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
    Ok(tool_success_result(ask_from_response(
        ask_response_from_result(&question, policy, result),
    )))
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
