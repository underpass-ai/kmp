use serde_json::{Value, json};

use crate::contract::handshake::CHRONOLOOM_APP_URI;
use crate::contract::tools::{
    app_view_undo, app_visual_projection, ask, forward, goto, ingest, inspect, near, rewind, trace,
    view_apply_intent, view_get_state, view_open, wake, write_memory,
};
use crate::tool_error::ToolErrorCode;

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
        if apps {
            tools.push(app_visual_projection::definition());
            tools.push(app_view_undo::definition());
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
            goto::definition(),
            near::definition(),
            rewind::definition(),
            forward::definition(),
            trace::definition(),
            inspect::definition(),
        ]
    })
}
