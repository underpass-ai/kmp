//! Reduced entries remain recoverable, while cursors still bind omitted content.
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn raw(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let reply = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,
        "method":"tools/call","params":{"name":tool,"arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("response");
    reviewed_writer::review_authored_write(
        server,
        serde_json::from_str::<Value>(&reply).expect("JSON")["result"].clone(),
    )
    .await
}

async fn call(server: &KernelMcpServer, tool: &str, args: Value) -> Value {
    let result = raw(server, tool, args).await;
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}

async fn seed(server: &KernelMcpServer, about: &str) -> Value {
    call(server, "kmp_write_memory", json!({"about":about,"actor":"test",
        "observed_at":"2026-09-01T10:00:00Z","idempotency_key":format!("{about}:fields:seed"),
        "labels":{"project":["shared"],"source":["S1"]},"memories":[
            {"id":"source","kind":"observation","summary":"S1 records the complete source. ".repeat(70),
             "evidence":"S1: complete source record, preserved verbatim."},
            {"id":"decision","kind":"decision","summary":"S2 selects local storage. ".repeat(70),
             "observed_at":"2026-09-01T11:00:00Z","labels":{"source":["S2"]},
             "evidence":"S2 selects local storage because S1 requires it.",
             "connect_to":[{"ref":"@source","rel":"chosen_because","class":"motivational",
                "why":"S1 motivates the local storage choice.","evidence":"S2 explicitly cites S1.","confidence":"high"}]}
        ]})).await
}

fn query() -> Value {
    json!({"about":"project:fields","axis":"observed",
        "interval":{"start":"2026-09-01T00:00:00Z","end":"2026-09-02T00:00:00Z"},
        "dimensions":{"selectors":[{"key":"project","op":"in","values":["shared"]}]},
        "include":{"evidence":false,"relations":false},"limit":{"entries":10},
        "budget":{"max_bytes":100000}})
}

#[tokio::test]
async fn reduced_entries_recover_complete_bodies_with_original_scope_and_clock() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    seed(&server, "project:fields").await;
    seed(&server, "project:other").await;
    let mut args = query();
    args["dimensions"]["scope"] = json!("abouts");
    args["dimensions"]["abouts"] = json!(["project:fields", "project:other"]);
    let full = call(&server, "kmp_forward", args.clone()).await;
    args["fields"] = json!(["coordinates"]);
    let reduced = call(&server, "kmp_forward", args.clone()).await;
    assert_eq!(reduced["entries"].as_array().expect("entries").len(), 4);
    assert_eq!(
        reduced["selection"]["fields"],
        json!({
        "included":["ref","kind","coordinates"],"omitted":["text","metadata"]})
    );
    assert!(reduced.to_string().len() < full.to_string().len());
    for (small, original) in reduced["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .zip(full["entries"].as_array().expect("entries"))
    {
        assert_eq!(small["ref"], original["ref"]);
        assert_eq!(small["kind"], original["kind"]);
        assert_eq!(small["coordinates"], original["coordinates"]);
        assert!(small.get("text").is_none() && small.get("metadata").is_none());
        let action = &small["detail_action"];
        for key in [
            "about",
            "dimensions",
            "axis",
            "interval",
            "include",
            "budget",
        ] {
            assert_eq!(action["arguments"][key], args[key], "{key}");
        }
        let expanded = call(
            &server,
            action["tool"].as_str().expect("tool"),
            action["arguments"].clone(),
        )
        .await;
        assert_eq!(expanded["page"]["has_more"], false);
        assert_eq!(expanded["entries"], full["entries"]);
        assert!(
            expanded["entries"]
                .as_array()
                .expect("entries")
                .contains(original)
        );
    }
}

#[tokio::test]
async fn empty_fields_keep_identity_and_do_not_silently_remove_requested_proof_or_raw_audit() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let written = seed(&server, "project:fields").await;
    for (tool, cursor) in [
        ("kmp_goto", "at"),
        ("kmp_near", "around"),
        ("kmp_rewind", "from"),
    ] {
        let mut args = query();
        args[cursor] = json!({"ref":written["local_refs"]["decision"]});
        args["include"] = json!({"evidence":true,"relations":true,"raw_refs":true});
        let full = call(&server, tool, args.clone()).await;
        args["fields"] = json!([]);
        let reduced = call(&server, tool, args).await;
        assert_eq!(reduced["proof"], full["proof"]);
        assert_eq!(reduced["raw_refs"], full["raw_refs"]);
        assert!(!full["raw_refs"].as_array().expect("raw refs").is_empty());
        for entry in reduced["entries"].as_array().expect("entries") {
            let keys: Vec<_> = entry
                .as_object()
                .expect("entry")
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(keys, ["detail_action", "kind", "ref"]);
            let action = &entry["detail_action"];
            let expanded = call(
                &server,
                action["tool"].as_str().expect("tool"),
                action["arguments"].clone(),
            )
            .await;
            let body = expanded["entries"]
                .as_array()
                .expect("entries")
                .iter()
                .find(|body| body["ref"] == entry["ref"])
                .expect("expanded entry");
            assert!(body["text"].is_string());
        }
    }
}

#[tokio::test]
async fn projected_pages_bind_omitted_content_and_fields_but_allow_a_larger_budget() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let written = seed(&server, "project:fields").await;
    let mut args = query();
    args["fields"] = json!([]);
    args["page"] = json!({"entries":1});
    args["budget"]["max_bytes"] = json!(10000);
    let first = call(&server, "kmp_forward", args.clone()).await;
    assert_eq!(first["page"]["returned"], 1);
    assert!(first.to_string().len() <= 10000);
    let next = first["next_actions"][0]["arguments"].clone();
    let mut resized = next.clone();
    resized["budget"]["max_bytes"] = json!(20000);
    let second = call(&server, "kmp_forward", resized).await;
    assert_eq!(second["page"]["offset"], 1);
    assert_ne!(second["entries"][0]["ref"], first["entries"][0]["ref"]);
    let mut changed = next.clone();
    changed["fields"] = json!(["text"]);
    assert_eq!(
        raw(&server, "kmp_forward", changed).await["structuredContent"]["feedback"][0]["code"],
        "READ_SELECTION_CHANGED"
    );
    call(&server,"kmp_write_memory",json!({"about":"project:fields","actor":"test",
        "observed_at":"2026-09-02T00:00:00Z","idempotency_key":"fields:change",
        "search_summaries":[{"ref":written["local_refs"]["source"],"summary_en":"S1 records the complete source and its local storage requirement."}]})).await;
    let error = raw(&server, "kmp_forward", next).await;
    assert_eq!(
        error["structuredContent"]["feedback"][0]["code"],
        "READ_SELECTION_CHANGED"
    );
    let action = &error["structuredContent"]["feedback"][0]["action"];
    assert_eq!(action["arguments"]["fields"], json!([]));
    let fresh = call(&server, "kmp_forward", action["arguments"].clone()).await;
    assert_eq!(
        fresh["entries"][0], first["entries"][0],
        "even unchanged visible entries must restart"
    );
}

#[tokio::test]
async fn invalid_field_selections_explain_the_allowed_vocabulary() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    seed(&server, "project:fields").await;
    for fields in [
        json!(["unknown"]),
        json!(["text", "text"]),
        json!(null),
        json!("text"),
    ] {
        let mut args = query();
        args["fields"] = fields;
        let result = raw(&server, "kmp_forward", args).await;
        assert_eq!(result["isError"], true, "{result}");
        let feedback = &result["structuredContent"]["feedback"][0];
        assert_eq!(feedback["code"], "READ_INVALID_FIELDS");
        assert_eq!(
            feedback["allowed"],
            json!(["ref", "kind", "text", "coordinates", "metadata"])
        );
    }
}

#[tokio::test]
async fn expanding_a_ref_preserves_distinct_membership_clocks() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let coordinate = |lane: &str, at: &str| {
        json!({
        "dimension":"timeline","scope_id":lane,"observed_at":at})
    };
    call(&server,"kmp_ingest",json!({"about":"project:fields",
        "idempotency_key":"fields:clocks","memory":{
            "dimensions":[{"kind":"timeline","id":"a"},{"kind":"timeline","id":"b"}],
            "entries":[
                {"id":"project:fields:observation:multi","kind":"observation","text":"Two placements of the same memory.",
                 "coordinates":[coordinate("a","2026-09-01T09:00:00Z"),coordinate("b","2026-09-01T11:00:00Z")]},
                {"id":"project:fields:observation:middle","kind":"observation","text":"Another memory between them.",
                 "coordinates":[coordinate("a","2026-09-01T10:00:00Z")]}
            ],"evidence":[],"relations":[]}})).await;
    let mut args = query();
    args["dimensions"] = json!({"mode":"only","include":["timeline"]});
    for tool in ["kmp_forward", "kmp_rewind"] {
        let full = call(&server, tool, args.clone()).await;
        assert_eq!(full["entries"].as_array().expect("entries").len(), 2);
        let mut selected = args.clone();
        selected["fields"] = json!(["coordinates"]);
        let reduced = call(&server, tool, selected).await;
        for (entry, original) in reduced["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .zip(full["entries"].as_array().expect("entries"))
        {
            let detail = call(
                &server,
                entry["detail_action"]["tool"].as_str().expect("tool"),
                entry["detail_action"]["arguments"].clone(),
            )
            .await;
            assert_eq!(detail["entries"], full["entries"]);
            assert!(
                detail["entries"]
                    .as_array()
                    .expect("entries")
                    .contains(original)
            );
        }
    }
}

#[path = "support/temporal_detail_checks.rs"]
mod temporal_detail_checks;
