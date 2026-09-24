use serde_json::{Value, json};

use crate::contract::handshake::CHRONOLOOM_APP_URI;
use crate::contract::tools::{
    app_view_take_control, app_view_undo, app_visual_projection, ask, condense, ingest, inspect,
    relabel, relate, summaries_audit, time, trace, view_apply_intent, view_get_state, view_open,
    wake, write_memory,
};
use crate::serving::tool_error_code::ToolErrorCode;

pub(crate) fn tools_list_result() -> Value {
    tools_list_result_with_apps(false)
}

/// Canonical model-facing names declared by this protocol build. Diagnostics
/// compare the surface they observe against names, never a count that goes
/// stale as honest tools are added.
pub(crate) fn declared_tool_names() -> Vec<String> {
    tools_list_result()["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect()
}

/// What `tools/list` advertises. The output schemas stay in the contract —
/// tests validate `structuredContent` against them and a host may opt in — but
/// by default they are not advertised: MCP makes `outputSchema` optional, and
/// at least one host (Claude Code) presents tools to the model with their
/// input parameters only, so the schemas were pure startup transport there.
pub(crate) fn advertised_tools_list(apps: bool, output_schemas: bool) -> Value {
    let result = tools_list_result_with_apps(apps);
    if output_schemas {
        result
    } else {
        without_output_schemas(result)
    }
}

/// Drops every tool's `outputSchema`, leaving the rest byte-for-byte intact.
pub(crate) fn without_output_schemas(mut result: Value) -> Value {
    for tool in result["tools"].as_array_mut().into_iter().flatten() {
        if let Some(tool) = tool.as_object_mut() {
            tool.remove("outputSchema");
        }
    }
    result
}

/// The full contract, output schemas included.
pub(crate) fn tools_list_result_with_apps(apps: bool) -> Value {
    let mut result = tools_list_core();
    if let Some(tools) = result["tools"].as_array_mut() {
        let mut open = view_open::definition();
        if apps {
            open["_meta"] = json!({
                "ui": {
                    "resourceUri": CHRONOLOOM_APP_URI,
                    "visibility": ["model", "app"]
                }
            });
        }
        tools.push(open);
        tools.push(view_apply_intent::definition());
        tools.push(view_get_state::definition());
        tools.push(crate::contract::tools::guide::definition());
        // Described once in the session instructions, not on every verb.
        for tool in tools.iter_mut().filter(|t| t["name"] != "kmp_guide") {
            tool["inputSchema"]["properties"]["context_id"] = json!({"type":"string"});
            tool["inputSchema"]["properties"]["purpose"] =
                json!({"type":"string","enum":["continue","audit","history","answer"]});
            if matches!(
                tool["name"].as_str(),
                Some("kmp_write_memory" | "kmp_relabel" | "kmp_condense")
            ) {
                tool["inputSchema"]["required"]
                    .as_array_mut()
                    .expect("required")
                    .retain(|field| field != "actor");
                tool["inputSchema"]["if"] = json!({"not":{"required":["actor"]}});
                tool["inputSchema"]["then"] = json!({"required":["context_id"]});
                tool["inputSchema"]["properties"]["actor"]["description"] = json!(
                    "Writer name; defaults to the persistent agent name when context_id is supplied. Required without a context."
                );
            }
            if crate::guidance::ReadContinuation::supports(
                tool["name"].as_str().unwrap_or_default(),
            ) {
                let name = tool["name"].as_str().unwrap_or_default().to_owned();
                let recall = matches!(name.as_str(), "kmp_wake" | "kmp_ask");
                let description = if recall {
                    "Returned call handle; use alone, or with page.repeat_core=true. Unavailable: submit the original call again."
                } else if name == "kmp_write_memory" {
                    "Returned call handle for the pending write and its review token; use alone. Resuming rechecks context before commit. Unavailable: submit the original call again."
                } else {
                    "Returned call handle; use alone. Unavailable: submit the original call again."
                };
                let schema = tool["inputSchema"].as_object_mut().expect("input schema");
                schema["properties"]["continuation"] = json!({"type":"string","pattern":"^read_[0-9a-fA-F]{32}$","description":description});
                // Keep the shared argument object at the root. Hosts that
                // render a root union from its branches alone otherwise lose
                // these properties and advertise unconstrained dictionaries.
                // The conditional preserves the same two disjoint call forms.
                let mut initial = json!({});
                for key in ["required", "anyOf", "oneOf", "allOf", "if", "then", "else"] {
                    if let Some(value) = schema.remove(key) {
                        initial[key] = value;
                    }
                }
                schema.insert("if".into(), json!({"required":["continuation"]}));
                // A handle stands for the whole call; Wake/Ask may add only
                // page.repeat_core beside it, which the server checks.
                schema.insert(
                    "then".into(),
                    json!({"maxProperties": if recall { 2 } else { 1 }}),
                );
                schema.insert("else".into(), initial);
            }
        }
        if apps {
            tools.push(app_visual_projection::definition());
            tools.push(crate::contract::tools::app_memory_nodes::definition());
            tools.push(app_view_undo::definition());
            tools.push(app_view_take_control::definition());
        }
    }
    result
}

/// The memory tools. Split from the view tools below so neither `json!`
/// expansion has to hold the whole surface at once.
fn tools_list_core() -> Value {
    json!({
        // The codes an agent may branch on, with what to do about each. They
        // were enumerated only in the source, while the skill told agents to
        // read the code — advice with nothing behind it in any host that does
        // not ship the skill.
        "_meta": {
            "kmp/errorCodes": ToolErrorCode::ALL
                .iter()
                .map(|code| json!({"code": code.as_str(), "means": code.guidance()}))
                .collect::<Vec<_>>()
        },
        "tools": [
            ingest::definition(),
            write_memory::definition(),
            wake::definition(),
            ask::definition(),
            relate::definition(),
            time::definition(),
            trace::definition(),
            inspect::definition(),
            relabel::definition(),
            condense::definition(),
            summaries_audit::definition(),
        ]
    })
}
