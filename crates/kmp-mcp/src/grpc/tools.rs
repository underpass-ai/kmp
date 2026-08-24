use serde_json::Value;

use crate::backend::KernelMcpGrpcTlsConfig;
use crate::grpc::channel::connect_memory_client;
use crate::grpc::requests::{
    ask_request_from_arguments, ingest_request_from_arguments, inspect_request_from_arguments,
    temporal_move_request_from_arguments, temporal_near_request_from_arguments,
    trace_request_from_arguments, wake_request_from_arguments,
};
use crate::grpc::temporal::{
    forward_request_from_temporal, goto_request_from_temporal, method_name,
    near_request_from_temporal, rewind_request_from_temporal, temporal_response_from_forward,
    temporal_response_from_goto, temporal_response_from_near, temporal_response_from_rewind,
};
use crate::ingest::build_ingest_plan;
use crate::kmp::{
    ask_from_response, dry_run_ingest_from_plan, enforce_inspect_output_budget,
    enforce_temporal_output_budget, ingest_from_response, inspect_from_response,
    temporal_from_response, trace_from_response, try_enforce_recall_output_budget,
    wake_from_response,
};
use crate::protocol::tool_success_result;
use crate::tool_error::{ToolError, ToolErrorCode};

/// gRPC already carries a status code, so this boundary reads that instead of
/// the sentence it produced. The server said what kind of failure it was; the
/// only way to lose that was to throw it away and guess later.
fn grpc_error(operation: &str, subject: &str) -> impl FnOnce(tonic::Status) -> ToolError {
    let context = format!("KernelMemoryService.{operation} failed for `{subject}`");
    move |status| {
        let code = match status.code() {
            tonic::Code::NotFound => ToolErrorCode::NotFound,
            tonic::Code::InvalidArgument | tonic::Code::OutOfRange => {
                ToolErrorCode::InvalidArgument
            }
            tonic::Code::AlreadyExists | tonic::Code::Aborted => ToolErrorCode::Conflict,
            tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => ToolErrorCode::Unavailable,
            _ => ToolErrorCode::BackendError,
        };
        ToolError::new(code, format!("{context}: {status}"))
    }
}

pub(super) async fn grpc_tool_result(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    name: &str,
    arguments: &Value,
) -> Result<Value, ToolError> {
    match name {
        "kernel_ingest" | "kernel_remember" | "kernel_ingest_context" => {
            grpc_ingest(endpoint, tls, arguments).await
        }
        "kernel_wake" => grpc_wake(endpoint, tls, arguments).await,
        "kernel_ask" => grpc_ask(endpoint, tls, arguments).await,
        "kernel_goto" => grpc_temporal_move(endpoint, tls, "goto", arguments).await,
        "kernel_near" => grpc_temporal_near(endpoint, tls, arguments).await,
        "kernel_rewind" => grpc_temporal_move(endpoint, tls, "rewind", arguments).await,
        "kernel_forward" => grpc_temporal_move(endpoint, tls, "forward", arguments).await,
        "kernel_trace" => grpc_trace(endpoint, tls, arguments).await,
        "kernel_inspect" => grpc_inspect(endpoint, tls, arguments).await,
        other => Err(ToolError::unknown_tool(format!(
            "unknown KMP tool `{other}`"
        ))),
    }
}

async fn grpc_ingest(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = ingest_request_from_arguments(arguments)?;
    if request.dry_run {
        let plan = build_ingest_plan(arguments)?;
        return Ok(tool_success_result(dry_run_ingest_from_plan(&plan)));
    }

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

async fn grpc_wake(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = wake_request_from_arguments(arguments)?;
    let about = request.about.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .wake(request)
        .await
        .map_err(grpc_error("Wake", &about))?
        .into_inner();

    Ok(tool_success_result(try_enforce_recall_output_budget(
        wake_from_response(response),
        arguments,
        1600,
    )?))
}

async fn grpc_ask(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = ask_request_from_arguments(arguments)?;
    let about = request.about.clone();
    let mut client = connect_memory_client(endpoint, tls)
        .await
        .map_err(ToolError::unavailable)?;
    let response = client
        .ask(request)
        .await
        .map_err(grpc_error("Ask", &about))?
        .into_inner();

    Ok(tool_success_result(try_enforce_recall_output_budget(
        ask_from_response(response),
        arguments,
        2400,
    )?))
}

async fn grpc_temporal_move(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    direction: &str,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = temporal_move_request_from_arguments(arguments, direction)?;
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
    let request = temporal_near_request_from_arguments(arguments)?;
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

async fn grpc_trace(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = trace_request_from_arguments(arguments)?;
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

    Ok(tool_success_result(trace_from_response(response)))
}

async fn grpc_inspect(
    endpoint: &str,
    tls: &KernelMcpGrpcTlsConfig,
    arguments: &Value,
) -> Result<Value, ToolError> {
    let request = inspect_request_from_arguments(arguments)?;
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
