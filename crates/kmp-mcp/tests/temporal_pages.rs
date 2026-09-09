//! Exercise returned actions against real memory, including complete proof recovery.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

async fn raw(server: &KernelMcpServer, tool: &str, args: Value) -> Value {
    let response = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,
        "method":"tools/call","params":{"name":tool,"arguments":args}})
            .to_string(),
        )
        .await
        .expect("response");
    serde_json::from_str::<Value>(&response).expect("JSON")["result"].clone()
}

async fn call(server: &KernelMcpServer, tool: &str, args: Value) -> Value {
    let response = raw(server, tool, args).await;
    assert_eq!(response["isError"], false, "{response}");
    response["structuredContent"].clone()
}

async fn seed(server: &KernelMcpServer) -> Value {
    call(server, "kmp_write_memory", json!({
        "about":"project:paging", "actor":"test", "observed_at":"2026-09-01T10:00:00Z",
        "labels":{"project":["p"],"source":["s1","s2","s3","s4","s5","s6"]},
        "idempotency_key":"paging:source", "memories":[
            {"id":"source","kind":"observation","summary":"Observed an interrupted delivery.",
             "evidence":"S1 records the exact interrupted delivery and its still unverified recovery. ".repeat(30)},
            {"id":"policy","kind":"decision","summary":"Retry delivery because S1 reports interruption.",
             "evidence":"S2 chooses retry because S1 reports interruption. ".repeat(30),
             "observed_at":"2026-09-02T10:00:00Z",
             "connect_to":[{"ref":"@source","rel":"chosen_because","class":"motivational",
                "why":"The interruption in S1 motivates the retry in S2.",
                "evidence":"S2 explicitly selects retry in response to S1.","confidence":"high"}]}
        ]
    })).await
}

fn query() -> Value {
    json!({"about":"project:paging","from":{"time":"2026-09-01T00:00:00Z"},
        "axis":"observed","dimensions":{"selectors":[{"key":"project","op":"in","values":["p"]}]},
        "include":{"evidence":true,"relations":true,"raw_refs":true},"limit":{"entries":10},
        "budget":{"max_bytes":10000}})
}

#[tokio::test]
async fn returned_actions_reconstruct_entries_and_proof_without_losing_selection() {
    let dir = tempfile::tempdir().expect("isolated store directory");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded server");
    seed(&server).await;
    let mut large = query();
    large["budget"]["max_bytes"] = json!(1_000_000);
    let full = call(&server, "kmp_forward", large).await;
    assert_eq!(full["page"]["has_more"], false);
    let mut page = call(&server, "kmp_forward", query()).await;
    assert_eq!(
        page["page"]["has_more"], true,
        "fixture needs proof pagination"
    );
    let mut reconstructed = full.clone();
    let pointers: Vec<_> = full["page"]["sections"]
        .as_object()
        .expect("expected response shape")
        .keys()
        .map(|name| format!("/{}", name.replace('.', "/")))
        .collect();
    for pointer in &pointers {
        *reconstructed.pointer_mut(pointer).expect("section") = json!([]);
    }
    let mut pages = 0;
    loop {
        assert!(
            page.to_string().len() <= 10000,
            "{} bytes",
            page.to_string().len()
        );
        assert_eq!(page["selection"], full["selection"]);
        assert_eq!(page["coverage"], full["coverage"]);
        for pointer in &pointers {
            reconstructed
                .pointer_mut(pointer)
                .expect("expected response shape")
                .as_array_mut()
                .expect("expected response shape")
                .extend(
                    page.pointer(pointer)
                        .expect("expected response shape")
                        .as_array()
                        .expect("expected response shape")
                        .iter()
                        .cloned(),
                );
        }
        pages += 1;
        if page["page"]["has_more"] == false {
            break;
        }
        assert!(pages < 50, "must advance");
        let action = &page["next_actions"][0];
        assert_eq!(action["tool"], "kmp_forward");
        assert_eq!(action["arguments"]["dimensions"], query()["dimensions"]);
        assert_eq!(action["arguments"]["axis"], "observed");
        let previous = page["page"]["offset"].as_u64().expect("count");
        page = call(
            &server,
            action["tool"].as_str().expect("string"),
            action["arguments"].clone(),
        )
        .await;
        assert!(page["page"]["offset"].as_u64().expect("count") > previous);
    }
    assert!(pages > 1);
    for pointer in &pointers {
        assert_eq!(
            reconstructed.pointer(pointer),
            full.pointer(pointer),
            "{pointer}"
        );
    }
    assert!(page["next_actions"].as_array().expect("array").is_empty());
}

#[tokio::test]
async fn changed_selection_or_proof_rejects_cursor_but_budget_can_change() {
    let dir = tempfile::tempdir().expect("isolated store directory");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded server");
    let written = seed(&server).await;
    let page = call(&server, "kmp_forward", query()).await;
    let args = page["next_actions"][0]["arguments"].clone();
    assert!(args["page"]["cursor"].is_string());
    let mut changed = args.clone();
    changed["axis"] = json!("ingested");
    let error = raw(&server, "kmp_forward", changed).await;
    assert_eq!(error["isError"], true);
    assert_eq!(error["structuredContent"]["error"]["code"], "conflict");
    let mut resized = args.clone();
    resized["budget"]["max_bytes"] = json!(50000);
    call(&server, "kmp_forward", resized).await;
    call(&server,"kmp_write_memory",json!({"about":"project:paging","actor":"test",
        "observed_at":"2026-09-03T00:00:00Z","idempotency_key":"paging:change",
        "search_summaries":[{"ref":written["local_refs"]["source"],"summary_en":"An interrupted delivery was observed and recovery is still unverified."}]})).await;
    let error = raw(&server, "kmp_forward", args).await;
    assert_eq!(error["isError"], true);
    let feedback = &error["structuredContent"]["feedback"][0];
    assert_eq!(feedback["code"], "READ_SELECTION_CHANGED");
    let action = &feedback["action"];
    assert!(action["arguments"].get("page").is_none());
    call(
        &server,
        action["tool"].as_str().expect("string"),
        action["arguments"].clone(),
    )
    .await;
}

#[tokio::test]
async fn an_indivisible_item_returns_a_retry_that_makes_progress() {
    let dir = tempfile::tempdir().expect("isolated store directory");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded server");
    seed(&server).await;
    let mut tiny = query();
    tiny["budget"]["max_bytes"] = json!(512);
    let page = call(&server, "kmp_forward", tiny).await;
    assert_eq!(page["page"]["returned"], 0);
    assert!(
        page["page"]["minimum_progress_bytes"]
            .as_u64()
            .expect("count")
            > 512
    );
    let action = &page["next_actions"][0];
    let retried = call(
        &server,
        action["tool"].as_str().expect("string"),
        action["arguments"].clone(),
    )
    .await;
    assert!(
        retried["page"]["returned"].as_u64().expect("count") > 0,
        "{retried}"
    );
    assert!(
        retried.to_string().len() as u64
            <= action["arguments"]["budget"]["max_bytes"]
                .as_u64()
                .expect("count")
    );
}

#[tokio::test]
async fn goto_and_near_offer_executable_navigation_after_the_packet_is_complete() {
    let dir = tempfile::tempdir().expect("isolated store directory");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded server");
    let written = seed(&server).await;
    for (tool, cursor, expected) in [
        ("kmp_goto", "at", vec!["kmp_rewind"]),
        ("kmp_near", "around", vec!["kmp_rewind", "kmp_forward"]),
    ] {
        let mut args = json!({"about":"project:paging","axis":"observed",
            "limit":{"entries":1},"window":{"before_entries":0,"after_entries":0},
            "include":{"evidence":false,"relations":false},"budget":{"max_bytes":100000}});
        args[cursor] = json!({"ref":written["local_refs"]["policy"]});
        let page = call(&server, tool, args.clone()).await;
        assert_eq!(page["page"]["has_more"], false, "response page is complete");
        assert_eq!(page["selection"]["has_more"], true, "more history remains");
        let actions = page["next_actions"].as_array().expect("array");
        assert_eq!(
            actions
                .iter()
                .map(|a| a["tool"].as_str().expect("string"))
                .collect::<Vec<_>>(),
            expected
        );
        for action in actions {
            assert_eq!(action["arguments"]["axis"], args["axis"]);
            assert_eq!(action["arguments"]["include"], args["include"]);
            assert!(action["arguments"].get(cursor).is_none());
            let moved = call(
                &server,
                action["tool"].as_str().expect("string"),
                action["arguments"].clone(),
            )
            .await;
            assert!(
                moved["entries"]
                    .as_array()
                    .expect("expected response shape")
                    .iter()
                    .all(|e| e["ref"] != written["local_refs"]["policy"])
            );
        }
    }
}
