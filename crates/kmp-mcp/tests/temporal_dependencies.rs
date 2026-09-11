//! Related sources remain complete, scoped and clock-bound across native pages.
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn raw(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let response = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0", "id":1,
        "method":"tools/call", "params":{"name":tool,"arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("response");
    reviewed_writer::review_authored_write(
        server,
        serde_json::from_str::<Value>(&response).expect("JSON")["result"].clone(),
    )
    .await
}

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let response = raw(server, tool, arguments).await;
    assert_eq!(response["isError"], false, "{response}");
    response["structuredContent"].clone()
}

async fn seed(server: &KernelMcpServer) -> Value {
    call(server, "kmp_write_memory", json!({
        "about":"project:dependencies", "actor":"test",
        "observed_at":"2026-09-08T11:00:00Z", "idempotency_key":"dependencies:source",
        "labels":{"project":["release"],"workshop":["Alba"]},
        "memories":[
            {"id":"alias", "kind":"observation", "observed_at":"2026-09-01T09:00:00Z",
             "summary":"Elena uses the name Nora in Alba. ".repeat(40),
             "evidence":"Register S1: Elena Vega is called Nora in workshop Alba. ".repeat(30),
             "labels":{"source":["S1"]}},
            {"id":"role", "kind":"constraint", "summary":"Nora in Alba is accountable for R19.",
             "occurred_at":"2026-09-10T10:00:00Z", "labels":{"source":["S2"]},
             "evidence":"Notice S2: Nora in Alba is accountable for R19. ".repeat(30),
             "connect_to":[{"ref":"@alias", "rel":"same_entity_as", "class":"evidential",
                "why":"The workshop register identifies the Nora named by the responsibility notice.",
                "evidence":"Both sources explicitly identify the same Nora within Alba.", "confidence":"high"}]}
        ]})).await
}

fn query() -> Value {
    json!({"about":"project:dependencies", "at":{"time":"2026-09-10T10:01:00Z"},
        "axis":"observed", "include":{"dependencies":true,"raw_refs":true},
        "limit":{"entries":1}, "budget":{"max_bytes":1_000_000}})
}

#[tokio::test]
async fn dependency_option_recovers_old_source_without_changing_history_selection() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let receipt = seed(&server).await;
    let with = call(&server, "kmp_goto", query()).await;
    let mut plain = query();
    plain["include"] = json!({"evidence":true,"relations":true,"raw_refs":true});
    let without = call(&server, "kmp_goto", plain).await;
    for key in ["entries", "coverage", "raw_refs"] {
        assert_eq!(with[key], without[key], "{key}");
    }
    assert!(without["proof"].get("groups").is_none());
    assert!(without["page"]["sections"].get("proof.groups").is_none());
    assert_eq!(with["entries"][0]["ref"], receipt["local_refs"]["role"]);
    assert_eq!(
        with["proof"]["groups"][0]["member_refs"],
        json!([
            receipt["local_refs"]["role"],
            receipt["local_refs"]["alias"]
        ])
    );
    let extra = &with["proof"]["entries"][0];
    let canonical = call(
        &server,
        "kmp_forward",
        json!({"about":"project:dependencies",
        "axis":"observed", "interval":{"start":"2026-09-01T00:00:00Z"},
        "budget":{"max_bytes":100000}}),
    )
    .await;
    assert_eq!(
        extra, &canonical["entries"][0],
        "dependency body and coordinate order match history"
    );
    assert_eq!(extra["ref"], receipt["local_refs"]["alias"]);
    assert_eq!(
        extra["text"],
        "Elena uses the name Nora in Alba. ".repeat(40).trim()
    );
    assert!(
        extra["coordinates"]
            .as_array()
            .expect("coordinates")
            .iter()
            .all(|coordinate| coordinate["observed_at"] == "2026-09-01T09:00:00Z")
    );
    assert!(
        with["proof"]["evidence"]
            .as_array()
            .expect("evidence")
            .iter()
            .any(|item| item["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("Register S1:")))
    );
    assert_eq!(with["proof"]["groups"][0]["unavailable_in_selection"], 0);
}

#[tokio::test]
async fn explicit_clock_and_label_filters_do_not_admit_a_related_but_ineligible_source() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let receipt = seed(&server).await;
    let mut clock = query();
    clock["axis"] = json!("occurred");
    let response = call(&server, "kmp_goto", clock).await;
    assert_eq!(
        response["proof"]["groups"][0]["member_refs"],
        json!([receipt["local_refs"]["role"]])
    );
    assert_eq!(
        response["proof"]["groups"][0]["unavailable_in_selection"],
        1
    );
    assert!(
        response["proof"]["entries"]
            .as_array()
            .expect("dependencies")
            .is_empty()
    );
    assert!(
        !response["proof"]["evidence"]
            .to_string()
            .contains("Register S1:")
    );
    let mut filtered = query();
    filtered["dimensions"] = json!({"selectors":[{"key":"source","op":"in","values":["S2"]}]});
    let response = call(&server, "kmp_goto", filtered).await;
    assert_eq!(
        response["proof"]["groups"][0]["member_refs"],
        json!([receipt["local_refs"]["role"]])
    );
    assert!(
        !response["proof"]["evidence"]
            .to_string()
            .contains("Register S1:")
    );
}

#[tokio::test]
async fn pages_reconstruct_dependency_bodies_and_links_even_with_reduced_entry_fields() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    seed(&server).await;
    let mut args = query();
    args["fields"] = json!([]);
    let full = call(&server, "kmp_goto", args.clone()).await;
    assert!(full["entries"][0].get("text").is_none());
    assert!(full["proof"]["entries"][0]["text"].is_string());
    let pointers: Vec<_> = full["page"]["sections"]
        .as_object()
        .expect("sections")
        .keys()
        .map(|name| format!("/{}", name.replace('.', "/")))
        .collect();
    let mut recovered = full.clone();
    for pointer in &pointers {
        *recovered.pointer_mut(pointer).expect("section") = json!([]);
    }
    args["budget"]["max_bytes"] = json!(10000);
    let mut page = call(&server, "kmp_goto", args).await;
    assert_eq!(page["page"]["has_more"], true, "fixture must require pages");
    for attempt in 0..50 {
        assert_eq!(page["selection"], full["selection"]);
        for pointer in &pointers {
            recovered
                .pointer_mut(pointer)
                .expect("section")
                .as_array_mut()
                .expect("array")
                .extend(
                    page.pointer(pointer)
                        .expect("section")
                        .as_array()
                        .expect("array")
                        .iter()
                        .cloned(),
                );
        }
        if page["page"]["has_more"] == false {
            break;
        }
        assert!(attempt < 49, "must finish response pagination");
        let action = &page["next_actions"][0];
        assert_eq!(action["arguments"]["include"]["dependencies"], true);
        let offset = page["page"]["offset"].as_u64().expect("offset");
        let returned = page["page"]["returned"].as_u64().expect("returned");
        page = call(
            &server,
            action["tool"].as_str().expect("tool"),
            action["arguments"].clone(),
        )
        .await;
        assert!(
            page["page"]["offset"].as_u64().expect("offset") > offset
                || (returned == 0 && page["page"]["returned"].as_u64().expect("returned") > 0)
        );
    }
    for pointer in pointers {
        if pointer == "/entries" {
            // Detail actions carry each call's budget; stored entry content does not.
            for entry in recovered["entries"].as_array_mut().expect("entries") {
                entry
                    .as_object_mut()
                    .expect("entry")
                    .remove("detail_action");
            }
            let mut expected = full["entries"].clone();
            for entry in expected.as_array_mut().expect("entries") {
                entry
                    .as_object_mut()
                    .expect("entry")
                    .remove("detail_action");
            }
            assert_eq!(recovered["entries"], expected);
            continue;
        }
        assert_eq!(
            recovered.pointer(&pointer),
            full.pointer(&pointer),
            "{pointer}"
        );
    }
}

#[tokio::test]
async fn forward_rewind_and_near_keep_dependency_expansion_when_selecting_positions() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    seed(&server).await;
    let goto = call(&server, "kmp_goto", query()).await;
    for (tool, cursor_key, time) in [
        ("kmp_forward", "from", "2026-09-02T00:00:00Z"),
        ("kmp_rewind", "from", "2026-09-10T10:01:00Z"),
        ("kmp_near", "around", "2026-09-10T10:01:00Z"),
    ] {
        let mut args = query();
        args.as_object_mut().expect("args").remove("at");
        args[cursor_key] = json!({"time":time});
        args["window"] = json!({"before_entries":1,"after_entries":0});
        let response = call(&server, tool, args).await;
        assert_eq!(
            response["proof"]["groups"], goto["proof"]["groups"],
            "{tool}"
        );
        assert_eq!(
            response["proof"]["entries"], goto["proof"]["entries"],
            "{tool}"
        );
    }
}

#[tokio::test]
async fn changed_dependency_invalidates_cursor_and_conflicting_include_returns_error() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let receipt = seed(&server).await;
    let mut args = query();
    args["budget"]["max_bytes"] = json!(10000);
    let first = call(&server, "kmp_goto", args).await;
    let continuation = first["next_actions"][0]["arguments"].clone();
    assert!(continuation["page"]["cursor"].is_string());
    call(&server, "kmp_write_memory", json!({"about":"project:dependencies", "actor":"test",
        "observed_at":"2026-09-10T10:01:00Z", "idempotency_key":"dependencies:summary",
        "search_summaries":[{"ref":receipt["local_refs"]["alias"],
            "summary_en":"The scoped register identifies Elena Vega as Nora within workshop Alba."}]})).await;
    let error = raw(&server, "kmp_goto", continuation).await;
    assert_eq!(error["isError"], true);
    assert_eq!(
        error["structuredContent"]["feedback"][0]["code"],
        "READ_SELECTION_CHANGED"
    );
    let mut conflict = query();
    conflict["include"]["evidence"] = json!(false);
    assert_eq!(raw(&server, "kmp_goto", conflict).await["isError"], true);
}
