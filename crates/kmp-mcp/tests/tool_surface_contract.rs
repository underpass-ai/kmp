//! The classification every semantic input field on the advertised surface
//! carries.
//!
//! This reads `tools/list` as a client does, through the crate's public
//! accessor, rather than the module that assembles it. The property under test
//! is about the published document — that no verb grew an argument nobody
//! classified as either a typed gRPC field or an explicit helper — so reading
//! it from outside is what makes the check mean what it says.

use serde_json::Value;

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

    let contract = kmp_mcp::kmp_mcp_tools_list_result_with_apps(false);
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
            "budget",
            "depth",
            "dimensions",
            "intent",
            "page",
            "role"
        ])
    );
    assert_eq!(
        keys(schema("kmp_ask")),
        expected(&[
            "about",
            "answer_policy",
            "budget",
            "depth",
            "dimensions",
            "page",
            "question",
        ])
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
