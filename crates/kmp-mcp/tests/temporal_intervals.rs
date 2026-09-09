//! Native interval reads preserve their bounds and keep earlier proof without later knowledge.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let reply = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,
        "method":"tools/call","params":{"name":tool,"arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("response");
    let value: Value = serde_json::from_str(&reply).expect("JSON");
    assert_eq!(value["result"]["isError"], false, "{value}");
    value["result"]["structuredContent"].clone()
}

async fn seed(server: &KernelMcpServer) -> Value {
    call(server, "kmp_write_memory", json!({
        "about":"project:interval", "actor":"test", "observed_at":"2026-09-01T14:00:00Z",
        "idempotency_key":"interval:source", "labels":{"project":["p"]},
        "memories":[
            {"id":"reason","kind":"constraint","summary":"S0 requires offline storage.",
             "evidence":"S0 requires data to remain on this device.","observed_at":"2026-09-01T09:00:00Z"},
            {"id":"decision","kind":"decision","summary":"S1 selects local SQLite because S0 requires offline storage.",
             "evidence":"S1 chooses SQLite in response to S0.","observed_at":"2026-09-01T10:00:00Z",
             "valid_from":"2026-09-01T10:00:00Z","valid_until":"2026-09-01T11:30:00Z",
             "connect_to":[{"ref":"@reason","rel":"chosen_because","class":"motivational",
                "why":"The offline constraint motivates this decision.","evidence":"S1 explicitly cites S0.","confidence":"high"}]},
            {"id":"tie","kind":"observation","summary":"S2 also arrives at exactly ten.",
             "evidence":"S2 is recorded at the inclusive start.","observed_at":"2026-09-01T10:00:00Z"},
            {"id":"end","kind":"observation","summary":"S3 arrives at exactly twelve.",
             "evidence":"S3 is recorded at the exclusive end.","observed_at":"2026-09-01T12:00:00Z"},
            {"id":"late","kind":"observation","summary":"S4 later verifies S1.",
             "evidence":"S4 reports successful offline operation at thirteen.","observed_at":"2026-09-01T13:00:00Z",
             "connect_to":[{"ref":"@decision","rel":"supports","class":"evidential",
                "why":"The later report supports the selected offline plan.","evidence":"S4 explicitly verifies S1.","confidence":"high"}]},
            {"id":"replacement","kind":"decision","summary":"S5 replaces S1 at fourteen.",
             "evidence":"S5 replaces the earlier decision.","observed_at":"2026-09-01T14:00:00Z",
             "connect_to":[{"ref":"@decision","rel":"supersedes","class":"evidential",
                "why":"S5 replaces the previous plan.","evidence":"S5 explicitly replaces S1.","confidence":"high"}]}
        ]
    })).await
}

fn query() -> Value {
    json!({"about":"project:interval","axis":"observed",
        "interval":{"start":"2026-09-01T10:00:00Z","end":"2026-09-01T12:00:00Z"},
        "dimensions":{"mode":"only","include":["project"]},"limit":{"entries":1},
        "include":{"evidence":true,"relations":true,"raw_refs":true},"budget":{"max_bytes":50000}})
}

#[tokio::test]
async fn direct_intervals_follow_returned_calls_in_both_directions_without_boundary_loss() {
    let dir = tempfile::tempdir().expect("isolated directory");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let written = seed(&server).await;
    for tool in ["kmp_forward", "kmp_rewind"] {
        let mut args = query();
        args["page"] = json!({"entries":2});
        let mut refs = Vec::new();
        let mut calls = 0;
        loop {
            let result = call(&server, tool, args).await;
            assert_eq!(result["temporal"]["interval"], query()["interval"]);
            refs.extend(
                result["entries"]
                    .as_array()
                    .expect("entries")
                    .iter()
                    .map(|e| e["ref"].clone()),
            );
            calls += 1;
            let actions = result["next_actions"].as_array().expect("actions");
            if actions.is_empty() {
                break;
            }
            assert!(calls < 50, "returned calls must advance");
            assert_eq!(actions[0]["tool"], tool);
            args = actions[0]["arguments"].clone();
            assert_eq!(args["interval"], query()["interval"]);
            assert_eq!(args["axis"], query()["axis"]);
            assert_eq!(args["dimensions"], query()["dimensions"]);
        }
        refs.sort_by_key(Value::to_string);
        let mut expected = vec![
            written["local_refs"]["decision"].clone(),
            written["local_refs"]["tie"].clone(),
        ];
        expected.sort_by_key(Value::to_string);
        assert_eq!(refs, expected);
    }
}

#[tokio::test]
async fn interval_proof_keeps_earlier_reasons_and_excludes_later_knowledge_and_replacement() {
    let dir = tempfile::tempdir().expect("isolated directory");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let written = seed(&server).await;
    let mut args = query();
    args["limit"]["entries"] = json!(10);
    let result = call(&server, "kmp_forward", args).await;
    assert_eq!(result["page"]["has_more"], false);
    assert_eq!(result["selection"]["has_more"], false);
    let proof = &result["proof"];
    assert!(
        proof["path"]
            .as_array()
            .expect("path")
            .iter()
            .any(|r| r["rel"] == "chosen_because" && r["to"] == written["local_refs"]["reason"])
    );
    assert!(
        proof["superseded"]
            .as_array()
            .expect("superseded")
            .is_empty()
    );
    assert!(
        proof["expired"]
            .as_array()
            .expect("expired")
            .iter()
            .any(|e| e["ref"] == written["local_refs"]["decision"])
    );
    assert_eq!(proof["interval"]["start"], Value::Null);
    assert_eq!(proof["interval"]["end"], "2026-09-01T12:00:00Z");
    let text = proof.to_string();
    for excluded in ["late", "replacement", "end"] {
        assert!(
            !text.contains(written["local_refs"][excluded].as_str().expect("ref")),
            "{proof}"
        );
    }
    let mut until_expiry = query();
    until_expiry["limit"]["entries"] = json!(10);
    until_expiry["interval"]["end"] = json!("2026-09-01T11:30:00Z");
    let before_expiry = call(&server, "kmp_forward", until_expiry).await;
    assert!(
        before_expiry["proof"]["expired"]
            .as_array()
            .expect("expired")
            .is_empty(),
        "expiry at the exclusive end has not happened within this read"
    );
}

#[tokio::test]
async fn a_later_link_between_old_memories_does_not_enter_an_earlier_interval() {
    let dir = tempfile::tempdir().expect("isolated directory");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let written = seed(&server).await;
    call(&server, "kmp_ingest", json!({
        "about":"project:interval", "idempotency_key":"interval:late-link",
        "memory":{"dimensions":[{"id":"p","kind":"project"}],
            "entries":[{"id":"project:interval:observation:review-late","kind":"observation",
                "text":"S6: the review at thirteen identifies S2 as support for S1.",
                "coordinates":[{"dimension":"project","scope_id":"p","observed_at":"2026-09-01T13:00:00Z"}]}],
            "evidence":[],"relations":[{
            "from":written["local_refs"]["tie"], "to":written["local_refs"]["decision"],
            "rel":"supports", "class":"evidential", "confidence":"high",
            "why":"The review received at thirteen establishes that S2 supports S1.",
            "evidence":"S6: at thirteen, the reviewer identifies S2 as support for S1.",
            "coordinate":{"dimension":"project","scope_id":"p","observed_at":"2026-09-01T13:00:00Z"}
        }]}
    })).await;
    let mut args = query();
    args["limit"]["entries"] = json!(10);
    args["interval"]["end"] = json!("2026-09-01T13:01:00Z");
    let later = call(&server, "kmp_forward", args.clone()).await;
    let is_late_link = |relation: &Value| {
        relation["from"] == written["local_refs"]["tie"]
            && relation["to"] == written["local_refs"]["decision"]
            && relation["rel"] == "supports"
    };
    assert!(
        later["proof"]["path"]
            .as_array()
            .expect("path")
            .iter()
            .any(is_late_link)
    );
    for end in ["2026-09-01T12:00:00Z", "2026-09-01T13:00:00Z"] {
        args["interval"]["end"] = json!(end);
        let earlier = call(&server, "kmp_forward", args.clone()).await;
        assert!(
            !earlier["proof"]["path"]
                .as_array()
                .expect("path")
                .iter()
                .any(is_late_link),
            "a relation learned at thirteen is outside the exclusive end {end}: {}",
            earlier["proof"]["path"]
        );
    }
}

#[tokio::test]
async fn an_open_end_returns_history_but_does_not_assert_that_nothing_expired() {
    let dir = tempfile::tempdir().expect("isolated directory");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let written = seed(&server).await;
    let mut arguments = query();
    arguments["interval"]
        .as_object_mut()
        .expect("interval")
        .remove("end");
    arguments["limit"]["entries"] = json!(20);
    let result = call(&server, "kmp_forward", arguments).await;
    assert!(
        result["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .any(|entry| entry["ref"] == written["local_refs"]["decision"])
    );
    assert_eq!(result["temporal"]["interval"]["end"], Value::Null);
    assert!(
        result["proof"]["missing"]
            .as_array()
            .expect("missing")
            .contains(&json!("expiry_boundary"))
    );
    assert!(
        result["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .is_some_and(|text| text.contains("interval.end")))
    );
}
