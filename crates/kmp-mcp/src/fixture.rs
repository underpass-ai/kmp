use serde_json::Value;

use crate::backend::{KernelMcpToolBackend, KernelMcpToolFuture};
use crate::contract::validate_required_arguments;
use crate::grpc::requests::visual_projection_request_from_arguments;
use crate::ingest::build_ingest_plan;
use crate::kmp::try_enforce_recall_output_budget;
use crate::serving::{app_data_success_result, tool_success_result};
use crate::tool_error::ToolError;

// The fixture backend answers with the reference examples from the contract.
// They are embedded from `fixtures/`, a vendored copy of
// `api/examples/kernel/v1beta1/kmp` — a published crate can only embed what
// ships inside it, and `scripts/ci/check-vendored-contract.sh` keeps the copy
// honest.
const INGEST_RESPONSE_FIXTURE: &str =
    include_str!("../fixtures/kernel/v1beta1/kmp/ingest.response.json");
const WAKE_RESPONSE_FIXTURE: &str =
    include_str!("../fixtures/kernel/v1beta1/kmp/wake.response.json");
const ASK_RESPONSE_FIXTURE: &str = include_str!("../fixtures/kernel/v1beta1/kmp/ask.response.json");
const GOTO_RESPONSE_FIXTURE: &str =
    include_str!("../fixtures/kernel/v1beta1/kmp/goto.response.json");
const NEAR_RESPONSE_FIXTURE: &str =
    include_str!("../fixtures/kernel/v1beta1/kmp/near.response.json");
const REWIND_RESPONSE_FIXTURE: &str =
    include_str!("../fixtures/kernel/v1beta1/kmp/rewind.response.json");
const FORWARD_RESPONSE_FIXTURE: &str =
    include_str!("../fixtures/kernel/v1beta1/kmp/forward.response.json");
const TRACE_RESPONSE_FIXTURE: &str =
    include_str!("../fixtures/kernel/v1beta1/kmp/trace.response.json");
const INSPECT_RESPONSE_FIXTURE: &str =
    include_str!("../fixtures/kernel/v1beta1/kmp/inspect.response.json");

#[derive(Clone, Copy, Debug, Default)]
pub struct FixtureKernelMcpBackend;

impl KernelMcpToolBackend for FixtureKernelMcpBackend {
    fn backend_name(&self) -> &'static str {
        "fixture"
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        Box::pin(async move { fixture_tool_result(name, arguments) })
    }
}

pub(crate) fn fixture_tool_result(name: &str, arguments: &Value) -> Result<Value, ToolError> {
    match name {
        "kmp_ingest" | "kernel_remember" | "kernel_ingest_context" => {
            build_ingest_plan(arguments)?;
            read_fixture_tool_result(arguments, &[], INGEST_RESPONSE_FIXTURE)
        }
        "kmp_wake" => {
            read_recall_fixture_tool_result(arguments, &["about"], WAKE_RESPONSE_FIXTURE, 1600)
        }
        "kmp_ask" => read_recall_fixture_tool_result(
            arguments,
            &["about", "question"],
            ASK_RESPONSE_FIXTURE,
            2400,
        ),
        "kmp_goto" => read_fixture_tool_result(arguments, &["about"], GOTO_RESPONSE_FIXTURE),
        "kmp_near" => read_fixture_tool_result(arguments, &["about"], NEAR_RESPONSE_FIXTURE),
        "kmp_rewind" => read_fixture_tool_result(arguments, &["about"], REWIND_RESPONSE_FIXTURE),
        "kmp_forward" => read_fixture_tool_result(arguments, &["about"], FORWARD_RESPONSE_FIXTURE),
        "kmp_trace" => read_fixture_tool_result(arguments, &["from", "to"], TRACE_RESPONSE_FIXTURE),
        "kmp_inspect" => read_fixture_tool_result(arguments, &["ref"], INSPECT_RESPONSE_FIXTURE),
        "kmp_view_read_projection" => {
            visual_projection_request_from_arguments(arguments)
                .map_err(ToolError::invalid_argument)?;
            Ok(app_data_success_result(serde_json::json!({
                "contract": "kmp.visual.projection.v1",
                "about": arguments["about"],
                "axis": arguments.get("axis").and_then(Value::as_str).unwrap_or("occurred"),
                "level_of_detail": arguments.get("lod").and_then(Value::as_str).unwrap_or("atlas"),
                "range": {"from": arguments["from"], "to": arguments["to"]},
                "bins": [], "clusters": [], "entries": [], "relations": [], "metrics": [],
                "included_dimensions": [], "missing_dimensions": [],
                "revision": 0, "content_hash": "",
                "page": {"returned": 0, "total": 0, "has_more": false, "next_cursor": null},
                "truncated": false, "missing": []
            })))
        }
        other => Err(ToolError::unknown_tool(format!(
            "unknown KMP tool `{other}`"
        ))),
    }
}

fn read_recall_fixture_tool_result(
    arguments: &Value,
    required_arguments: &[&str],
    fixture: &str,
    default_tokens: u32,
) -> Result<Value, ToolError> {
    validate_required_arguments(arguments, required_arguments)?;
    let structured_content = serde_json::from_str::<Value>(fixture)
        .map_err(|error| format!("fixture response is invalid JSON: {error}"))?;
    Ok(tool_success_result(try_enforce_recall_output_budget(
        structured_content,
        arguments,
        default_tokens,
    )?))
}

fn read_fixture_tool_result(
    arguments: &Value,
    required_arguments: &[&str],
    fixture: &str,
) -> Result<Value, ToolError> {
    validate_required_arguments(arguments, required_arguments)?;
    let structured_content = serde_json::from_str::<Value>(fixture)
        .map_err(|error| format!("fixture response is invalid JSON: {error}"))?;
    Ok(tool_success_result(structured_content))
}
