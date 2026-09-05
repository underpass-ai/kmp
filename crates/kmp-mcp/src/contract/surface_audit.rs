//! The assembled surface, audited as one document: shapes, schema
//! coverage, vocabulary, and the error codes an agent may branch on.
#![cfg(test)]

use serde_json::Value;

#[allow(unused_imports)]
use crate::contract::handshake::*;
#[allow(unused_imports)]
use crate::contract::registry::*;
#[allow(unused_imports)]
use crate::contract::tools::write_memory::{read_context_schema, write_memory_schema};
#[allow(unused_imports)]
use crate::contract::validator::*;

mod tests {
    #[allow(unused_imports)]
    use super::*;
    use crate::serving::tool_result::tool_error_result;
    use crate::serving::{ToolError, ToolErrorCode};
    use serde_json::json;

    #[test]
    fn former_tool_names_resolve_to_the_advertised_kmp_surface() {
        let former = [
            "kernel_ingest",
            "kernel_write_memory",
            "kernel_wake",
            "kernel_ask",
            "kernel_goto",
            "kernel_near",
            "kernel_rewind",
            "kernel_forward",
            "kernel_trace",
            "kernel_inspect",
        ];
        let tools = tools_list_result();
        let current = tools["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect::<Vec<_>>();

        // Every former name still resolves to something this surface
        // advertises. It is not an equality: the view tools were born with
        // their kmp_ names and never had a kernel_ one to rename.
        for name in former.map(canonical_tool_name) {
            assert!(
                current.contains(&name),
                "former name resolves to `{name}`, which the surface no longer advertises"
            );
        }
        assert!(current.iter().all(|name| name.starts_with("kmp_")));
    }

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
    fn initialize_result_reports_backend_metadata() {
        let result = initialize_result("stub", "mutual");

        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(result["metadata"]["backend"], "stub");
        assert_eq!(result["metadata"]["grpc_tls"], "mutual");
        let instructions = result["instructions"].as_str().expect("instructions");
        // Whichever routing mode this machine configured, its gate is served
        // ahead of the rules it scopes.
        let routing_rules = instructions
            .find("Temporal intent has precedence")
            .expect("routing rules");
        assert!(
            routing_rules > 0,
            "initialize must open with the memory-routing gate"
        );
        assert!(instructions.contains("Preserve evidence text"));
        assert!(instructions.contains("Refs are opaque identifiers"));
        assert!(instructions.contains("Never prefix or qualify it with an about"));
    }

    #[test]
    fn tools_list_result_exposes_expected_tool_shapes() {
        let result = tools_list_result();
        let tools = result["tools"]
            .as_array()
            .expect("tools should be an array");

        assert_eq!(tools.len(), 14, "eleven memory tools and three view tools");
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
        assert_eq!(tools[4]["name"], "kmp_relate");
        assert_eq!(tools[4]["inputSchema"]["required"][0], "about");
        assert_eq!(tools[5]["name"], "kmp_goto");
        assert_eq!(tools[5]["inputSchema"]["required"][1], "at");
        assert!(tools[5]["description"].as_str().is_some_and(|description| {
            description.contains("feeding page.next_cursor back to kmp_goto does not paginate")
        }));
        assert!(
            tools[5]["outputSchema"]["properties"]
                .get("next_action")
                .is_some()
        );
        assert!(
            tools[5]["outputSchema"]["properties"]["page"]["properties"]["next_cursor"]
                ["description"]
                .as_str()
                .is_some_and(|description| description.contains("Do not pass it back to `at.ref`"))
        );
        assert!(
            tools[6]["outputSchema"]["properties"]["page"]["properties"]["next_cursor"]
                ["description"]
                .as_str()
                .is_some_and(|description| description.contains("Do not pass it back to `around.ref`"))
        );
    }

    #[test]
    fn semantic_input_fields_have_a_typed_grpc_or_explicit_helper_classification() {
        fn keys(value: &Value) -> std::collections::BTreeSet<String> {
            value
                .get("properties")
                .and_then(Value::as_object)
                .expect("schema properties")
                .keys()
                .cloned()
                .collect()
        }

        fn expected(values: &[&str]) -> std::collections::BTreeSet<String> {
            values.iter().map(|value| (*value).to_string()).collect()
        }

        let contract = tools_list_result();
        let tools = contract["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| (tool["name"].as_str().expect("name"), tool))
            .collect::<std::collections::BTreeMap<_, _>>();
        let schema = |name: &str| &tools[name]["inputSchema"];

        assert_eq!(
            keys(schema("kmp_ingest")),
            expected(&[
                "about",
                "dry_run",
                "idempotency_key",
                "memory",
                "provenance"
            ])
        );
        assert_eq!(
            keys(schema("kmp_wake")),
            expected(&[
                "about",
                "as_of",
                "axis",
                "budget",
                "depth",
                "dimensions",
                "intent",
                "interval",
                "page",
                "role"
            ])
        );
        assert_eq!(
            keys(schema("kmp_ask")),
            expected(&[
                "about",
                "answer_policy",
                "as_of",
                "asked_as",
                "axis",
                "budget",
                "depth",
                "dimensions",
                "interval",
                "page",
                "question",
            ])
        );
        assert_eq!(
            keys(schema("kmp_relate")),
            expected(&["about", "axis", "budget", "dimensions", "interval", "page"])
        );
        for (name, cursor) in [
            ("kmp_goto", "at"),
            ("kmp_near", "around"),
            ("kmp_rewind", "from"),
            ("kmp_forward", "from"),
        ] {
            assert_eq!(
                keys(schema(name)),
                expected(&[
                    "about",
                    "axis",
                    "budget",
                    "depth",
                    "dimensions",
                    "include",
                    "limit",
                    "window",
                    cursor,
                ]),
                "{name} typed request crosswalk"
            );
            assert_eq!(
                keys(&schema(name)["properties"][cursor]),
                expected(&["ref", "sequence", "time"])
            );
        }
        assert_eq!(
            keys(schema("kmp_trace")),
            expected(&["about", "budget", "from", "goal", "page", "role", "to"])
        );
        assert_eq!(
            keys(schema("kmp_inspect")),
            expected(&["about", "budget", "include", "page", "ref"])
        );

        // These shared shapes are one-to-one with MemoryBudget,
        // DimensionSelection and PageRequest. `depth` at the tool root is an
        // explicit compatibility alias for MemoryBudget.depth; Trace.role is
        // an alias for TraceRequest.goal. Inspect.budget and Inspect.page are
        // transport-only result projection and never cross the gRPC request;
        // its budget may only carry max_bytes.
        let wake_properties = &schema("kmp_wake")["properties"];
        assert_eq!(
            keys(&wake_properties["budget"]),
            expected(&["depth", "detail", "max_bytes", "max_entries", "tokens"])
        );
        assert_eq!(
            keys(&wake_properties["dimensions"]),
            expected(&["abouts", "exclude", "include", "mode", "scope", "scope_ids"])
        );
        assert_eq!(
            keys(&wake_properties["page"]),
            expected(&["cursor", "entries"])
        );
        assert_eq!(
            keys(&schema("kmp_inspect")["properties"]["budget"]),
            expected(&["max_bytes"])
        );
        assert_eq!(
            keys(&schema("kmp_inspect")["properties"]["page"]),
            expected(&["cursor"])
        );

        // The writer is the sole non-RPC tool: every field belongs to the
        // validated helper contract and its output is pinned to canonical
        // Ingest by the writer compilation and four-path parity tests.
        assert_eq!(
            keys(schema("kmp_write_memory")),
            expected(&[
                "about",
                "actor",
                "connect_to",
                "current",
                "idempotency_key",
                "intent",
                "labels",
                "occurred_at",
                "observed_at",
                "options",
                "rank",
                "read_context",
                "scope",
                "semantic_delta",
                "source_kind",
                "valid_from",
                "valid_until",
            ])
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
            AskResponse, IngestResponse, InspectResponse, RelateResponse, TemporalMoveResponse,
            TraceResponse, WakeResponse,
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
                crate::projection::ingest_from_response(IngestResponse::default()),
            ),
            (
                "kmp_wake",
                crate::projection::enforce_recall_output_budget(
                    crate::projection::wake_from_response(WakeResponse::default()),
                    &recall_arguments,
                    1_600,
                ),
            ),
            (
                "kmp_ask",
                crate::projection::enforce_recall_output_budget(
                    crate::projection::ask_from_response(AskResponse::default()),
                    &recall_arguments,
                    2_400,
                ),
            ),
            (
                "kmp_goto",
                crate::projection::temporal_from_response(TemporalMoveResponse::default()),
            ),
            (
                "kmp_relate",
                crate::projection::relate_from_response(RelateResponse::default()),
            ),
            (
                "kmp_trace",
                crate::projection::trace_from_response(TraceResponse::default()),
            ),
            (
                "kmp_inspect",
                crate::projection::inspect_from_response(InspectResponse::default()),
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
            "labels",
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

    /// The property the substring matcher could not have. Same words, two
    /// codes, because the producer chose and the words were never consulted.
    #[test]
    fn the_code_comes_from_the_producer_and_not_from_the_message() {
        let phrased_like_a_bad_argument =
            "the store must be migrated before it can be opened; this is invalid";
        assert_eq!(
            tool_error_result(&ToolError::backend(phrased_like_a_bad_argument))["structuredContent"]
                ["error"]["code"],
            "backend_error"
        );
        assert_eq!(
            tool_error_result(&ToolError::invalid_argument(phrased_like_a_bad_argument))["structuredContent"]
                ["error"]["code"],
            "invalid_argument"
        );
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
