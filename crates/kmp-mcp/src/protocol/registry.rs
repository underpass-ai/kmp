//! The advertised surface, assembled once.
//!
//! One concept: which tools this build declares, in what order, and how a
//! caller's argument schema is looked up. No tool is described here — each
//! module under `tools` owns its own description — so adding a verb is one
//! line in this file and one new module, never an edit spread across a
//! monolith.

use serde_json::{Value, json};

use crate::protocol::chronoloom_app::CHRONOLOOM_APP_URI;
use crate::protocol::tools;
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
    let mut declared = memory_tools();
    let mut open = tools::view_open::definition();
    if apps {
        open["_meta"] = json!({
            "ui": {
                "resourceUri": CHRONOLOOM_APP_URI,
                "visibility": ["model", "app"]
            }
        });
    }
    declared.push(open);
    declared.push(tools::view_apply_intent::definition());
    declared.push(tools::view_get_state::definition());
    if apps {
        declared.push(tools::app_projection::definition());
        declared.push(tools::app_view_undo::definition());
    }
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
        "tools": declared
    })
}

/// The memory tools, in the order the surface advertises them. The view tools
/// are appended by the caller because two of them are conditional on the host
/// having declared MCP App support.
fn memory_tools() -> Vec<Value> {
    let mut declared = vec![
        tools::ingest::definition(),
        tools::write_memory::definition(),
        tools::wake::definition(),
        tools::ask::definition(),
    ];
    declared.extend(tools::temporal::definitions());
    declared.push(tools::trace::definition());
    declared.push(tools::inspect::definition());
    declared
}

/// The schemas, built once.
///
/// This runs on every tool call, and `tools_list_result()` builds the whole
/// full tool document — relation vocabulary included — from scratch each time.
/// Rebuilding a document that cannot change, per call, to read one field of
/// it, is a cost with nothing on the other side of it.
pub(in crate::protocol) fn tool_input_schema(tool: &str) -> Option<&'static Value> {
    if tool == "kmp_view_read_projection" {
        static APP_VISUAL_SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        return Some(APP_VISUAL_SCHEMA.get_or_init(tools::app_projection::input_schema));
    }
    if tool == "kmp_view_undo" {
        static APP_UNDO_SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        return Some(
            APP_UNDO_SCHEMA
                .get_or_init(|| tools::app_view_undo::definition()["inputSchema"].clone()),
        );
    }
    static SCHEMAS: std::sync::OnceLock<std::collections::BTreeMap<String, Value>> =
        std::sync::OnceLock::new();
    SCHEMAS
        .get_or_init(|| {
            tools_list_result()["tools"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|definition| {
                    Some((
                        definition["name"].as_str()?.to_string(),
                        definition["inputSchema"].clone(),
                    ))
                })
                .collect()
        })
        .get(tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The view half is small on purpose, and every one of its moves is
    /// under concurrency control: an agent that cannot be told "the human
    /// moved first" would eventually yank the loom out from under someone.
    #[test]
    fn the_view_tools_are_declarative_idempotent_and_conflict_aware() {
        let result = tools_list_result();
        let tools = result["tools"].as_array().expect("tools");
        let view = |name: &str| {
            tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("`{name}` is not advertised"))
                .clone()
        };

        let intent = view("kmp_view_apply_intent");
        assert_eq!(intent["inputSchema"]["required"][0], "idempotency_key");
        let properties = &intent["inputSchema"]["properties"];
        assert!(properties.get("expected_revision").is_some());
        assert!(properties.get("explanation").is_some());
        // Semantic vocabulary only — no coordinates, no pixels, no code.
        for forbidden in ["x", "y", "zoom_level", "camera", "html", "script"] {
            assert!(
                properties.get(forbidden).is_none(),
                "`{forbidden}` would make the agent drive pixels instead of meaning"
            );
        }
        let axis = &properties["focus"]["properties"]["time_range"]["properties"]["axis"]["enum"];
        assert_eq!(axis[0], "occurred", "the clock is chosen, never assumed");
        assert_eq!(
            properties["projection"]["properties"]["semantic_zoom"]["enum"],
            json!(["atlas", "episode", "moment"]),
            "evidence is opened by selecting an entry, not requested as a zoom rung"
        );

        assert_eq!(view("kmp_view_open")["inputSchema"]["required"][0], "about");
        for name in ["kmp_view_open", "kmp_view_get_state"] {
            assert!(
                view(name)["outputSchema"]["properties"]["url"]["description"]
                    .as_str()
                    .expect("viewer URL description")
                    .contains("capability"),
                "{name} must advertise the handoff link"
            );
        }
        assert!(
            view("kmp_view_apply_intent")["outputSchema"]["properties"]
                .get("url")
                .is_none(),
            "only discovery tools promise the capability handoff"
        );
        assert!(
            view("kmp_view_get_state")["description"]
                .as_str()
                .expect("description")
                .contains("never pixels")
        );
    }

    #[test]
    fn tools_list_result_exposes_expected_tool_shapes() {
        let result = tools_list_result();
        let tools = result["tools"]
            .as_array()
            .expect("tools should be an array");

        assert_eq!(tools.len(), 13, "ten memory tools and three view tools");
        assert_eq!(tools[0]["name"], "kmp_ingest");
        assert_eq!(tools[0]["inputSchema"]["required"][1], "memory");
        assert_eq!(tools[1]["name"], "kmp_write_memory");
        let writer_description = tools[1]["description"]
            .as_str()
            .expect("writer description");
        assert!(writer_description.contains("Normal writes are one call"));
        assert!(writer_description.contains("validation failures write nothing"));
        assert!(writer_description.contains("explicitly requested preview"));
        assert_eq!(tools[1]["inputSchema"]["required"][1], "intent");
        assert_eq!(
            tools[1]["inputSchema"]["properties"]["connect_to"]["items"]["properties"]["rel"]["enum"]
                [0],
            "follows"
        );
        assert!(
            tools[1]["inputSchema"]["properties"]
                .get("read_context")
                .is_some()
        );
        let why_description = tools[1]["inputSchema"]["properties"]["connect_to"]["items"]
            ["properties"]["why"]["description"]
            .as_str()
            .expect("writer why carries operational guidance");
        let evidence_description =
            tools[1]["inputSchema"]["properties"]["connect_to"]["items"]["properties"]["evidence"]
                ["description"]
                .as_str()
                .expect("writer evidence carries operational guidance");
        assert!(why_description.contains("specific semantic connection"));
        assert!(why_description.contains("later reader"));
        assert!(evidence_description.contains("concrete observation or source"));
        assert!(evidence_description.contains("relation rationale"));
        assert_eq!(tools[2]["name"], "kmp_wake");
        assert_eq!(tools[2]["inputSchema"]["required"][0], "about");
        assert_eq!(tools[2]["_meta"]["anthropic/maxResultSizeChars"], 10_000);
        assert_eq!(
            tools[2]["inputSchema"]["properties"]["budget"]["properties"]["max_bytes"]["minimum"],
            512
        );
        assert!(tools[2]["inputSchema"]["properties"].get("page").is_some());
        assert_eq!(tools[3]["name"], "kmp_ask");
        assert_eq!(tools[3]["inputSchema"]["required"][1], "question");
        assert_eq!(tools[3]["_meta"]["anthropic/maxResultSizeChars"], 10_000);
        assert!(tools[3]["inputSchema"]["properties"].get("page").is_some());
        assert!(
            tools[3]["inputSchema"]["properties"]
                .get("prefer")
                .is_none()
        );
        assert_eq!(tools[4]["name"], "kmp_goto");
        assert_eq!(tools[4]["inputSchema"]["required"][1], "at");
        assert!(tools[4]["description"].as_str().is_some_and(|description| {
            description.contains("feeding page.next_cursor back to kmp_goto does not paginate")
        }));
        assert!(
            tools[4]["outputSchema"]["properties"]
                .get("next_action")
                .is_some()
        );
        assert!(
            tools[4]["outputSchema"]["properties"]["page"]["properties"]["next_cursor"]
                ["description"]
                .as_str()
                .is_some_and(|description| description.contains("Do not pass it back to `at.ref`"))
        );
        assert!(
            tools[5]["outputSchema"]["properties"]["page"]["properties"]["next_cursor"]
                ["description"]
                .as_str()
                .is_some_and(|description| description.contains("Do not pass it back to `around.ref`"))
        );
    }

    #[test]
    fn every_tool_describes_the_response_it_returns() {
        let tools = tools_list_result();
        let tools = tools["tools"].as_array().expect("tools");

        for tool in tools {
            let name = tool["name"].as_str().expect("tool name");
            let schema = &tool["outputSchema"];
            assert_eq!(schema["type"], "object", "{name} output is an object");
            assert_eq!(
                schema["additionalProperties"], false,
                "{name} must not grow an unexplained response field"
            );
            let properties = schema["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{name} output properties"));
            assert!(!properties.is_empty(), "{name} describes its response");
            for (field, field_schema) in properties {
                let explained = field_schema
                    .get("description")
                    .and_then(Value::as_str)
                    .is_some_and(|description| !description.trim().is_empty())
                    || field_schema
                        .get("properties")
                        .and_then(Value::as_object)
                        .is_some_and(|nested| !nested.is_empty());
                assert!(explained, "{name}.outputSchema.{field} has a meaning");
            }
        }
    }

    #[test]
    fn output_schemas_cover_the_mapper_top_level_fields() {
        use kmp_proto::v1beta1::{
            AskResponse, IngestResponse, InspectResponse, TemporalMoveResponse, TraceResponse,
            WakeResponse,
        };

        let tools = tools_list_result();
        let schemas = tools["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| (tool["name"].as_str().expect("name"), &tool["outputSchema"]))
            .collect::<std::collections::BTreeMap<_, _>>();
        let recall_arguments = json!({});
        let samples = [
            (
                "kmp_ingest",
                crate::kmp::ingest_from_response(IngestResponse::default()),
            ),
            (
                "kmp_wake",
                crate::kmp::enforce_recall_output_budget(
                    crate::kmp::wake_from_response(WakeResponse::default()),
                    &recall_arguments,
                    1_600,
                ),
            ),
            (
                "kmp_ask",
                crate::kmp::enforce_recall_output_budget(
                    crate::kmp::ask_from_response(AskResponse::default()),
                    &recall_arguments,
                    2_400,
                ),
            ),
            (
                "kmp_goto",
                crate::kmp::temporal_from_response(TemporalMoveResponse::default()),
            ),
            (
                "kmp_trace",
                crate::kmp::trace_from_response(TraceResponse::default()),
            ),
            (
                "kmp_inspect",
                crate::kmp::inspect_from_response(InspectResponse::default()),
            ),
        ];

        for (name, sample) in samples {
            let described = schemas[name]["properties"]
                .as_object()
                .expect("schema properties");
            for field in sample.as_object().expect("mapped response").keys() {
                assert!(
                    described.contains_key(field),
                    "{name} returns `{field}` but its outputSchema does not describe it"
                );
            }
        }

        for name in ["kmp_near", "kmp_rewind", "kmp_forward"] {
            assert_eq!(
                schemas[name]["properties"]
                    .as_object()
                    .expect("temporal properties")
                    .keys()
                    .collect::<Vec<_>>(),
                schemas["kmp_goto"]["properties"]
                    .as_object()
                    .expect("temporal properties")
                    .keys()
                    .collect::<Vec<_>>()
            );
        }
        for field in [
            "accepted",
            "dry_run",
            "summary",
            "generated_refs",
            "relations",
            "relation_quality",
            "relation_quality_metrics",
            "diagnostics",
            "next_suggested_reads",
        ] {
            assert!(
                schemas["kmp_write_memory"]["properties"]
                    .get(field)
                    .is_some(),
                "writer output describes `{field}`"
            );
        }
    }

    #[test]
    fn each_verb_publishes_the_defaults_its_backend_uses() {
        let tools = tools_list_result();
        let tools = tools["tools"].as_array().expect("tools");
        let defaults = [
            ("kmp_wake", 1_600, 2),
            ("kmp_ask", 2_400, 2),
            ("kmp_goto", 2_400, 3),
            ("kmp_near", 2_400, 3),
            ("kmp_rewind", 2_400, 3),
            ("kmp_forward", 2_400, 3),
            ("kmp_trace", 1_600, 1),
        ];

        for (name, tokens, depth) in defaults {
            let tool = tools
                .iter()
                .find(|tool| tool["name"] == name)
                .unwrap_or_else(|| panic!("{name}"));
            let budget = &tool["inputSchema"]["properties"]["budget"]["properties"];
            assert_eq!(budget["tokens"]["default"], tokens, "{name} tokens");
            assert_eq!(budget["depth"]["default"], depth, "{name} depth");
            assert_eq!(budget["max_bytes"]["default"], 10_000);
            assert_eq!(budget["detail"]["default"], "balanced");
        }
    }

    #[test]
    fn the_surface_enumerates_every_error_code_it_can_return() {
        let listed = tools_list_result()["_meta"]["kmp/errorCodes"]
            .as_array()
            .expect("the codes an agent may branch on are part of the surface")
            .iter()
            .map(|entry| entry["code"].as_str().expect("a code").to_string())
            .collect::<Vec<_>>();

        for code in ToolErrorCode::ALL {
            assert!(
                listed.contains(&code.as_str().to_string()),
                "`{code}` can be returned and is not in tools/list"
            );
        }
        assert_eq!(listed.len(), ToolErrorCode::ALL.len());
    }
}
