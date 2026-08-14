use serde_json::Value;

use crate::args::validate_required_arguments;
use crate::backend::{KernelMcpToolBackend, KernelMcpToolFuture};
use crate::ingest::build_ingest_plan;
use crate::protocol::tool_success_result;

const INGEST_RESPONSE_FIXTURE: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/ingest.response.json");
const WAKE_RESPONSE_FIXTURE: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/wake.response.json");
const ASK_RESPONSE_FIXTURE: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/ask.response.json");
const GOTO_RESPONSE_FIXTURE: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/goto.response.json");
const NEAR_RESPONSE_FIXTURE: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/near.response.json");
const REWIND_RESPONSE_FIXTURE: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/rewind.response.json");
const FORWARD_RESPONSE_FIXTURE: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/forward.response.json");
const TRACE_RESPONSE_FIXTURE: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/trace.response.json");
const INSPECT_RESPONSE_FIXTURE: &str =
    include_str!("../../../api/examples/kernel/v1beta1/kmp/inspect.response.json");

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

pub(crate) fn fixture_tool_result(name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "kernel_ingest" | "kernel_remember" | "kernel_ingest_context" => {
            build_ingest_plan(arguments)?;
            read_fixture_tool_result(arguments, &[], INGEST_RESPONSE_FIXTURE)
        }
        "kernel_wake" => read_fixture_tool_result(arguments, &["about"], WAKE_RESPONSE_FIXTURE),
        "kernel_ask" => {
            read_fixture_tool_result(arguments, &["about", "question"], ASK_RESPONSE_FIXTURE)
        }
        "kernel_goto" => read_fixture_tool_result(arguments, &["about"], GOTO_RESPONSE_FIXTURE),
        "kernel_near" => read_fixture_tool_result(arguments, &["about"], NEAR_RESPONSE_FIXTURE),
        "kernel_rewind" => read_fixture_tool_result(arguments, &["about"], REWIND_RESPONSE_FIXTURE),
        "kernel_forward" => {
            read_fixture_tool_result(arguments, &["about"], FORWARD_RESPONSE_FIXTURE)
        }
        "kernel_trace" => {
            read_fixture_tool_result(arguments, &["from", "to"], TRACE_RESPONSE_FIXTURE)
        }
        "kernel_inspect" => read_fixture_tool_result(arguments, &["ref"], INSPECT_RESPONSE_FIXTURE),
        other => Err(format!("unknown KMP tool `{other}`")),
    }
}

fn read_fixture_tool_result(
    arguments: &Value,
    required_arguments: &[&str],
    fixture: &str,
) -> Result<Value, String> {
    validate_required_arguments(arguments, required_arguments)?;
    let structured_content = serde_json::from_str::<Value>(fixture)
        .map_err(|error| format!("fixture response is invalid JSON: {error}"))?;
    Ok(tool_success_result(structured_content))
}
