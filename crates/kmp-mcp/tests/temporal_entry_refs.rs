//! Entry focus preserves the question time and expands proof outside that focus.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn raw(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let reply = server.handle_json_line(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":arguments}}).to_string()).await.expect("response");
    serde_json::from_str::<Value>(&reply).expect("json")["result"].clone()
}
async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let result = raw(server, tool, arguments).await;
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}
async fn seed(server: &KernelMcpServer) -> Value {
    call(
        server,
        "kmp_write_memory",
        serde_json::from_str(include_str!("fixtures/temporal_entry_refs/write.json"))
            .expect("fixture"),
    )
    .await["local_refs"]
        .clone()
}
fn query(reference: Value) -> Value {
    json!({"about":"project:seed-routing","at":{"time":"2026-09-10T10:00:00Z"},"axis":"observed","refs":[reference],"include":{"dependencies":true},"limit":{"entries":1},"budget":{"max_bytes":500000}})
}
#[tokio::test]
async fn chosen_entry_keeps_inclusive_cutoff_and_full_dependency_sources() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let refs = seed(&server).await;
    let focused = call(&server, "kmp_goto", query(refs["role"].clone())).await;
    assert_eq!(focused["entries"][0]["ref"], refs["role"]);
    assert_eq!(focused["proof"]["as_of"], "2026-09-10T10:00:00Z");
    assert_eq!(
        focused["selection"]["requested_refs"],
        json!([refs["role"]])
    );
    assert_eq!(focused["selection"]["matching_entries"], 1);
    assert_eq!(focused["selection"]["unmatched_ref_count"], 0);
    let extra = focused["proof"]["entries"]
        .as_array()
        .expect("dependencies");
    assert_eq!(extra.len(), 2);
    for name in ["alias", "boundary"] {
        assert!(extra.iter().any(|e| e["ref"] == refs[name]));
    }
    assert!(!focused["proof"].to_string().contains("FUTURE"));
    let mut all = query(refs["role"].clone());
    all.as_object_mut().expect("query").remove("refs");
    all["limit"]["entries"] = json!(20);
    let canonical = call(&server, "kmp_goto", all).await;
    for entry in extra {
        assert!(
            canonical["entries"]
                .as_array()
                .expect("entries")
                .contains(entry),
            "full stored source and coordinates"
        );
    }
}
#[tokio::test]
async fn refs_do_not_override_time_scope_or_hard_label_filters() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let refs = seed(&server).await;
    let mut outside: Value =
        serde_json::from_str(include_str!("fixtures/temporal_entry_refs/write.json"))
            .expect("fixture");
    outside["about"] = json!("project:outside");
    outside["idempotency_key"] = json!("outside:fixture");
    let external = call(&server, "kmp_write_memory", outside).await["local_refs"]["role"].clone();
    let mut args = query(refs["future"].clone());
    args["refs"] = json!([refs["future"], external, "unknown-memory-ref"]);
    let absent = call(&server, "kmp_goto", args).await;
    assert_eq!(absent["entries"], json!([]));
    assert!(absent["proof"].get("entries").is_none());
    assert_eq!(absent["proof"]["evidence"], json!([]));
    assert_eq!(absent["selection"]["unmatched_ref_count"], 3);
    let mut missing_clock = query(refs["role"].clone());
    missing_clock["axis"] = json!("occurred");
    let undated = call(&server, "kmp_goto", missing_clock).await;
    assert_eq!(undated["entries"], json!([]));
    assert_eq!(undated["selection"]["unmatched_ref_count"], 1);
    let mut args = query(refs["role"].clone());
    args["dimensions"] = json!({"selectors":[{"key":"source","op":"in","values":["S2"]}]});
    let filtered = call(&server, "kmp_goto", args).await;
    assert_eq!(filtered["entries"][0]["ref"], refs["role"]);
    assert_eq!(filtered["proof"]["entries"], json!([]));
}

#[tokio::test]
async fn focus_precedes_limits_and_history_navigation_retains_the_ref_set() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let refs = seed(&server).await;
    let mut args = query(refs["role"].clone());
    args["refs"] = json!([refs["role"], refs["boundary"]]);
    // Resolve an original cursor independently of the requested entry order.
    args["at"] = json!({"ref":refs["boundary"]});
    let first = call(&server, "kmp_goto", args).await;
    assert_eq!(first["entries"][0]["ref"], refs["boundary"]);
    assert_eq!(first["selection"]["matching_entries"], 2);
    assert_eq!(first["selection"]["unmatched_ref_count"], 0);
    assert_eq!(first["selection"]["has_more"], true);
    assert_eq!(first["page"]["has_more"], false);
    let action = &first["next_actions"][0];
    let next = call(
        &server,
        action["tool"].as_str().expect("tool"),
        action["arguments"].clone(),
    )
    .await;
    assert_eq!(next["entries"][0]["ref"], refs["role"]);
    assert_eq!(
        next["selection"]["requested_refs"],
        first["selection"]["requested_refs"]
    );
    assert_eq!(next["selection"]["has_more"], false);

    let mut args = query(refs["role"].clone());
    args["at"] = json!({"ref":refs["boundary"]});
    let old = call(&server, "kmp_goto", args).await;
    assert_eq!(old["entries"][0]["ref"], refs["role"]);
    assert_eq!(old["proof"]["as_of"], "2026-09-10T10:00:00Z");
}
#[tokio::test]
async fn invalid_explicit_ref_sets_are_rejected_instead_of_reading_everything() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    for refs in [
        json!([]),
        json!(["a", "a"]),
        json!([" "]),
        json!([" a"]),
        json!(null),
        json!("a"),
    ] {
        let mut args = query(json!("a"));
        args["refs"] = refs;
        assert_eq!(raw(&server, "kmp_goto", args).await["isError"], true);
    }
}
#[tokio::test]
async fn reference_focus_is_retained_by_pages_and_binds_the_cursor() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let refs = seed(&server).await;
    let full = call(&server, "kmp_goto", query(refs["role"].clone())).await;
    let mut args = query(refs["role"].clone());
    args["page"] = json!({"entries":1});
    let first = call(&server, "kmp_goto", args).await;
    assert_eq!(first["page"]["has_more"], true);
    let mut next = first["next_actions"][0]["arguments"].clone();
    // Bare response cursors test selection binding independently of saved handles.
    if next.get("continuation").is_some() {
        next = query(refs["role"].clone());
        next["page"] = json!({"cursor":first["page"]["next_cursor"],"entries":1});
    }
    let mut changed = next.clone();
    changed["refs"] = json!([refs["alias"]]);
    assert_eq!(raw(&server, "kmp_goto", changed).await["isError"], true);
    let mut pages = vec![first];
    for _ in 0..100 {
        let page = call(&server, "kmp_goto", next).await;
        let more = page["page"]["has_more"] == true;
        next = page["next_actions"][0]["arguments"].clone();
        pages.push(page);
        if !more {
            break;
        }
    }
    assert_eq!(pages.last().expect("pages")["page"]["has_more"], false);
    for section in full["page"]["sections"]
        .as_object()
        .expect("sections")
        .keys()
    {
        let ptr = format!("/{}", section.replace('.', "/"));
        let all = pages
            .iter()
            .flat_map(|p| {
                p.pointer(&ptr)
                    .and_then(Value::as_array)
                    .expect("section")
                    .clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            Value::Array(all),
            full.pointer(&ptr).expect("full section").clone(),
            "{section}"
        );
    }
}
