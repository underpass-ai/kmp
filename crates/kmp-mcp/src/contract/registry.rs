use serde_json::{Value, json};

use crate::contract::handshake::CHRONOLOOM_APP_URI;
use crate::contract::tools::{
    app_view_take_control, app_view_undo, app_visual_projection, ask, condense, forward, goto,
    ingest, inspect, near, relabel, relate, rewind, trace, view_apply_intent, view_get_state,
    view_open, wake, write_memory,
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
        for tool in tools.iter_mut().filter(|t| t["name"] != "kmp_guide") {
            tool["inputSchema"]["properties"]["context_id"] = json!({"type":"string","description":"Optional active context returned by kmp_guide; records use and enables concise guidance."});
            tool["inputSchema"]["properties"]["purpose"] = json!({"type":"string","enum":["continue","audit","history","answer"],"description":"Optional recommendation purpose with context_id. Omitted: continue the selected packet."});
            if matches!(
                tool["name"].as_str(),
                Some("kmp_write_memory" | "kmp_relabel" | "kmp_condense")
            ) {
                tool["inputSchema"]["required"]
                    .as_array_mut()
                    .expect("required")
                    .retain(|field| field != "actor");
                tool["inputSchema"]["anyOf"] =
                    json!([{"required":["actor"]},{"required":["context_id"]}]);
                tool["inputSchema"]["properties"]["actor"]["description"] = json!(
                    "Writer name; defaults to the persistent agent name when context_id is supplied. Required without a context."
                );
            }
            if crate::guidance::ReadContinuation::supports(
                tool["name"].as_str().unwrap_or_default(),
            ) {
                let schema = tool["inputSchema"].as_object_mut().expect("input schema");
                schema["properties"]["continuation"] = json!({"type":"string","pattern":"^read_[0-9a-fA-F]{32}$","description":"Returned call handle. Use alone on its verb. Preserves read selection or the exact pending write and review token. A write resume rechecks context before commit. Unavailable: submit the original call again."});
                let mut initial = json!({"not":{"required":["continuation"]}});
                for key in ["required", "anyOf", "oneOf", "allOf", "if", "then", "else"] {
                    if let Some(value) = schema.remove(key) {
                        initial[key] = value;
                    }
                }
                schema.insert(
                    "oneOf".into(),
                    json!([initial,{"required":["continuation"],"maxProperties":1}]),
                );
            }
        }
        if apps {
            tools.push(app_visual_projection::definition());
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
            goto::definition(),
            near::definition(),
            rewind::definition(),
            forward::definition(),
            trace::definition(),
            inspect::definition(),
            relabel::definition(),
            condense::definition(),
        ]
    })
}
