use crate::projection::relation_page_budget::RelationPageBudget;
use prost::Message;
use serde_json::Value;

use crate::projection::{
    ask_from_response, condense_from_response, enforce_inspect_output_budget,
    enforce_temporal_output_budget, ingest_from_response, inspect_from_response,
    relabel_from_response, relate_from_response, temporal_from_response, trace_from_response,
    visual_projection_from_response, wake_from_response,
};
use crate::serving::KernelMcpGrpcTlsConfig;
use crate::serving::adapters::grpc::channel::connect_memory_client;
use crate::serving::adapters::grpc::temporal::{
    forward_request_from_temporal, goto_request_from_temporal, method_name,
    near_request_from_temporal, rewind_request_from_temporal, temporal_response_from_forward,
    temporal_response_from_goto, temporal_response_from_near, temporal_response_from_rewind,
};
use crate::serving::adapters::tool_request_mapping::{
    AskRequestMapper, CondenseRequestMapper, IngestRequestMapper, InspectRequestMapper,
    NearRequestMapper, RelabelRequestMapper, RelateRequestMapper, TemporalMoveRequestMapper,
    TraceRequestMapper, VisualProjectionRequestMapper, WakeRequestMapper,
};
use crate::serving::{ToolError, ToolErrorCode};
use crate::serving::{app_data_success_result, tool_success_result};

/// gRPC already carries a status code, so this boundary reads that instead of
/// the sentence it produced. The server said what kind of failure it was; the
/// only way to lose that was to throw it away and guess later.
/// Frames a live-kernel failure the way the embedded path frames its own:
/// the operation and the subject in front, the code from the producer. Any
/// future backend should copy this shape so an agent reads one grammar.
fn grpc_error(operation: &str, subject: &str) -> impl FnOnce(tonic::Status) -> ToolError {
    let context = format!("KernelMemoryService.{operation} failed for `{subject}`");
    let is_recall = matches!(operation, "Wake" | "Ask");
    move |status| {
        if is_recall
            && !status.details().is_empty()
            && let Ok(detail) = kmp_proto::v1beta1::RecallCursorError::decode(status.details())
            && detail.restart.is_some()
        {
            return crate::projection::recall_error::cursor(detail);
        }
        let code = match status.code() {
            tonic::Code::NotFound => ToolErrorCode::NotFound,
            tonic::Code::InvalidArgument | tonic::Code::OutOfRange => {
                ToolErrorCode::InvalidArgument
            }
            tonic::Code::AlreadyExists | tonic::Code::Aborted => ToolErrorCode::Conflict,
            tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => ToolErrorCode::Unavailable,
            _ => ToolErrorCode::BackendError,
        };
        // `status` Display repeats the code this error already carries in
        // its typed field; the message alone is what a person reads.
        ToolError::new(code, format!("{context}: {}", status.message()))
    }
}

pub(super) async fn grpc_tool_result(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    name: &str,
    arguments: &Value,
) -> Result<Value, ToolError> {
    match name {
        "kmp_ingest" | "kernel_remember" | "kernel_ingest_context" => {
            grpc_ingest(endpoint, tls, arguments).await
        }
        "kmp_wake" => grpc_wake(endpoint, tls, arguments).await,
        "kmp_ask" => grpc_ask(endpoint, tls, arguments).await,
        "kmp_goto" => grpc_temporal_move(endpoint, tls, "goto", arguments).await,
        "kmp_near" => grpc_temporal_near(endpoint, tls, arguments).await,
        "kmp_rewind" => grpc_temporal_move(endpoint, tls, "rewind", arguments).await,
        "kmp_forward" => grpc_temporal_move(endpoint, tls, "forward", arguments).await,
        "kmp_relate" => grpc_relate(endpoint, tls, arguments).await,
        "kmp_trace" => grpc_trace(endpoint, tls, arguments).await,
        "kmp_inspect" => grpc_inspect(endpoint, tls, arguments).await,
        "kmp_relabel" => grpc_relabel(endpoint, tls, arguments).await,
        "kmp_condense" => grpc_condense(endpoint, tls, arguments).await,
        "kmp_view_read_projection" => grpc_visual_projection(endpoint, tls, arguments).await,
        // The audit is read off the store's own event log, including the
        // earlier revisions of every entry, and the kernel's gRPC surface
        // exposes no such stream. Saying so is honest; answering from the
        // projections would be a second, weaker reading wearing this tool's
        // name.
        "kmp_summaries_audit" => Err(ToolError::unavailable(
            "kmp_summaries_audit reads the store's own event log, which live gRPC mode does not \
             serve. Run it against the embedded store, or list what is owed there with `kmp-mcp \
             summaries pending`",
        )),
        other => Err(ToolError::unknown_tool(format!(
            "unknown KMP tool `{other}`"
        ))),
    }
}

async fn grpc_visual_projection(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = VisualProjectionRequestMapper::from_arguments(arguments)
        .map_err(ToolError::invalid_argument)?;
    let about = request.about.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .project_visual(request)
        .await
        .map_err(grpc_error("ProjectVisual", &about))?
        .into_inner();
    Ok(app_data_success_result(visual_projection_from_response(
        response,
    )))
}

async fn grpc_ingest(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        IngestRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;

    let about = request.about.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .ingest(request)
        .await
        .map_err(grpc_error("Ingest", &about))?
        .into_inner();

    Ok(tool_success_result(ingest_from_response(response)))
}

async fn grpc_relabel(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        RelabelRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let subject = format!("{}/{}", request.about, request.r#ref);
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .relabel(request)
        .await
        .map_err(grpc_error("Relabel", &subject))?
        .into_inner();
    Ok(tool_success_result(relabel_from_response(response)))
}

/// Authors one reader card against a live kernel.
///
/// The served kernel stamps authorship, so this carries no instant. The card
/// policy's refusals arrive as gRPC codes and are mapped by `grpc_error` like
/// every other typed failure; nothing here reads the sentence.
async fn grpc_condense(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        CondenseRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let subject = format!("{}/{}", request.about, request.r#ref);
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .condense(request)
        .await
        .map_err(grpc_error("Condense", &subject))?
        .into_inner();
    Ok(tool_success_result(condense_from_response(response)))
}

async fn grpc_wake(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        WakeRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let about = request.about.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .wake(request)
        .await
        .map_err(grpc_error("Wake", &about))?
        .into_inner();

    Ok(tool_success_result(wake_from_response(response)))
}

async fn grpc_ask(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        AskRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let about = request.about.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .ask(request)
        .await
        .map_err(grpc_error("Ask", &about))?
        .into_inner();

    Ok(tool_success_result(ask_from_response(response)))
}

async fn grpc_temporal_move(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    direction: &str,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = TemporalMoveRequestMapper::from_arguments(arguments, direction)
        .map_err(ToolError::invalid_argument)?;
    let about = request.about.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = match direction {
        "goto" => client
            .goto(goto_request_from_temporal(request))
            .await
            .map(|response| temporal_response_from_goto(response.into_inner())),
        "rewind" => client
            .rewind(rewind_request_from_temporal(request))
            .await
            .map(|response| temporal_response_from_rewind(response.into_inner())),
        "forward" => client
            .forward(forward_request_from_temporal(request))
            .await
            .map(|response| temporal_response_from_forward(response.into_inner())),
        _ => {
            return Err(ToolError::invalid_argument(format!(
                "unknown temporal direction `{direction}`"
            )));
        }
    }
    .map_err(grpc_error(method_name(direction), &about))?;

    Ok(tool_success_result(enforce_temporal_output_budget(
        temporal_from_response(response),
        arguments,
    )?))
}

async fn grpc_temporal_near(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        NearRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let about = request.about.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .near(near_request_from_temporal(request))
        .await
        .map_err(grpc_error("Near", &about))?
        .into_inner();

    Ok(tool_success_result(enforce_temporal_output_budget(
        temporal_from_response(temporal_response_from_near(response)),
        arguments,
    )?))
}

async fn grpc_relate(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        RelateRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let about = request.about.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .relate(request)
        .await
        .map_err(grpc_error("Relate", &about))?
        .into_inner();

    let fingerprint = response.selection_fingerprint.clone();
    Ok(tool_success_result(RelationPageBudget::Relate.apply(
        relate_from_response(response),
        arguments,
        &fingerprint,
    )?))
}

async fn grpc_trace(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        TraceRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let from = request.from.clone();
    let to = request.to.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .trace(request)
        .await
        .map_err(grpc_error("Trace", &format!("{from}` -> `{to}")))?
        .into_inner();

    let fingerprint = response.selection_fingerprint.clone();
    Ok(tool_success_result(RelationPageBudget::Trace.apply(
        trace_from_response(response),
        arguments,
        &fingerprint,
    )?))
}

async fn grpc_inspect(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request =
        InspectRequestMapper::from_arguments(arguments).map_err(ToolError::invalid_argument)?;
    let ref_id = request.r#ref.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .inspect(request)
        .await
        .map_err(grpc_error("Inspect", &ref_id))?
        .into_inner();

    Ok(tool_success_result(enforce_inspect_output_budget(
        inspect_from_response(response),
        arguments,
    )?))
}
