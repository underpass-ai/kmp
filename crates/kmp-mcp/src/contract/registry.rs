use serde_json::{Value, json};

use crate::contract::cross_field_rule::tagged;
use crate::contract::handshake::CHRONOLOOM_APP_URI;
use crate::contract::tools::{
    app_view_take_control, app_view_undo, app_visual_projection, ask, condense, curate, ingest,
    inspect, relabel, relate, summaries_audit, time, trace, view_apply_intent, view_get_state,
    view_open, wake, write_memory,
};

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
    let result = advertised_input_schemas(tools_list_result_with_apps(apps));
    if output_schemas {
        result
    } else {
        without_output_schemas(result)
    }
}

/// The input schemas as `tools/list` serves them: the published law
/// (`published_law`) without parameter prose, which `kmp_guide` serves from
/// this same contract (#850).
pub(crate) fn advertised_input_schemas(result: Value) -> Value {
    crate::contract::parameter_guide::without_parameter_descriptions(
        crate::contract::published_law::published(result),
    )
}

/// `openWorldHint: true` on `tools`, the ones this session's store opted
/// into TypeSafe Jev for (`KernelMcpToolBackend::open_world_tools`). The
/// contract declares every tool closed to the outside world; only a store's
/// opt-in changes that, and the store a session serves is fixed when it
/// starts, as is the configuration it read, so the list cannot change under
/// a running session.
pub(crate) fn with_open_world(mut result: Value, tools: &[&str]) -> Value {
    for tool in result["tools"].as_array_mut().into_iter().flatten() {
        if tool["name"]
            .as_str()
            .is_some_and(|name| tools.contains(&name))
        {
            tool["annotations"]["openWorldHint"] = Value::Bool(true);
        }
    }
    result
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
        for tool in tools.iter_mut() {
            let name = tool["name"].as_str().unwrap_or_default().to_owned();
            // A writer may omit actor: the server names the MCP client.
            if matches!(
                name.as_str(),
                "kmp_write_memory" | "kmp_relabel" | "kmp_condense"
            ) {
                tool["inputSchema"]["required"]
                    .as_array_mut()
                    .expect("required")
                    .retain(|field| field != "actor");
            }
            if crate::continuation::ReadContinuation::supports(&name) {
                let recall = matches!(name.as_str(), "kmp_wake" | "kmp_ask");
                let description = if recall {
                    "Returned call handle; send it alone or with arguments equal to its call's, or with page.repeat_core=true. Unavailable: submit the original call again."
                } else if name == "kmp_write_memory" {
                    "Returned call handle for the pending write and its review token; send it alone or with arguments equal to its call's. Resuming rechecks context before commit. Unavailable: submit the original call again."
                } else {
                    "Returned call handle; send it alone or with arguments equal to its call's. Unavailable: submit the original call again."
                };
                let schema = tool["inputSchema"].as_object_mut().expect("input schema");
                // A handle stands for the whole call. Beside it any declared
                // argument may be resent, with the value the call already
                // has: the server compares it with the retained call and
                // refuses a different one under this rule (read_continuations).
                schema["properties"]["continuation"] = json!({"type":"string","pattern":"^read_[0-9a-fA-F]{32}$","description":description});
                let alongside: Vec<String> = schema["properties"]
                    .as_object()
                    .map(|properties| properties.keys().cloned().collect())
                    .unwrap_or_default();
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
                // `tools/list` publishes the `if` and the `else`'s
                // `required`: what a first call needs. The rest is the
                // tagged cross-field rules (`cross_field_rule`).
                schema.insert("if".into(), json!({"required":["continuation"]}));
                schema.insert(
                    "then".into(),
                    tagged(
                        "continuation-alone",
                        json!({"propertyNames":{"enum":alongside}}),
                    ),
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
    crate::contract::timestamp_format::with_timestamp_formats(result)
}

/// The memory tools. Split from the view tools below so neither `json!`
/// expansion has to hold the whole surface at once.
fn tools_list_core() -> Value {
    json!({
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
            curate::definition(),
        ]
    })
}
