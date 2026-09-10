//! HTTP must authorize the resolved selection, including hidden raw/scope grants.
use super::*;

async fn native(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":arguments}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("reply");
    serde_json::from_str::<Value>(&response).expect("JSON")["result"].clone()
}

async fn send(
    path: &std::path::Path,
    identity: Identity,
    action: &Value,
    shared: bool,
) -> Response {
    let app = router(AppState::new(
        config(Duration::from_secs(5)),
        KernelMcpServer::embedded(path)
            .expect("server")
            .with_shared_passages(shared),
        Arc::new(FakeVerifier {
            result: Ok(identity),
        }),
    ));
    app.oneshot(request(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":action["tool"],"arguments":action["arguments"]}}))).await.expect("HTTP response")
}

#[tokio::test]
async fn retained_read_cannot_bypass_raw_about_reference_or_dimension_grants() {
    check_retained_grants(false).await;
}

#[tokio::test]
async fn shared_passages_cannot_bypass_retained_read_grants() {
    check_retained_grants(true).await;
}

async fn check_retained_grants(shared: bool) {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path())
        .expect("server")
        .with_shared_passages(shared);
    let guide: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../plugins/kmp/guide/guide.requests.json"
    ))
    .expect("guide");
    for args in guide {
        assert_eq!(native(&server, "kmp_ingest", args).await["isError"], false);
    }
    let registered = native(
        &server,
        "kmp_guide",
        json!({"registration_key":"http-retained"}),
    )
    .await;
    let context = registered["structuredContent"]["context_id"].clone();
    let written = native(&server, "kmp_write_memory", json!({"about":"project:kmp","context_id":context,"observed_at":"2026-09-09T10:00:00Z","idempotency_key":"http:retained:source","labels":{"task":["route"]},"memories":[{"id":"source","kind":"observation","summary":"The route opens on Tuesday.","evidence":"S1 records that the route opens on Tuesday. ".repeat(90)}]})).await;
    assert_eq!(written["isError"], false, "{written}");
    let first = native(&server, "kmp_inspect", json!({"about":"project:kmp","context_id":context,"ref":written["structuredContent"]["local_refs"]["source"],"include":{"raw":true},"budget":{"max_bytes":512}})).await;
    let action = first["structuredContent"]["next_actions"][0].clone();
    assert!(action["arguments"]["continuation"].is_string(), "{first}");
    let allowed = identity(&["kmp:read", "kmp:inspect:raw", "kmp:all-abouts"]);
    let mut wrong_about = allowed.clone();
    wrong_about.abouts = BTreeSet::from(["project:other".into()]);
    let mut wrong_ref = allowed.clone();
    wrong_ref.ref_prefixes = BTreeSet::from(["project:other:".into()]);
    let mut no_read = allowed.clone();
    no_read.scopes.remove("kmp:read");
    for denied in [identity(&["kmp:read"]), wrong_about, wrong_ref, no_read] {
        assert_eq!(
            send(dir.path(), denied, &action, shared).await.status(),
            StatusCode::FORBIDDEN
        );
    }
    let accepted = send(dir.path(), allowed.clone(), &action, shared).await;
    assert_eq!(accepted.status(), StatusCode::OK);
    let body = response_json(accepted).await;
    assert_eq!(body["result"]["isError"], false, "{body}");
    assert!(
        body["result"]["structuredContent"]["page"]["returned"]
            .as_u64()
            .expect("progress")
            > 0
    );

    // A wake cursor also keeps scope/all-abouts requirements hidden by the handle.
    let wake = native(&server, "kmp_wake", json!({"about":"project:kmp","context_id":context,"dimensions":{"scope":"all_abouts","scope_ids":["route"]},"budget":{"max_bytes":2048}})).await;
    let action = wake["structuredContent"]["projection"]["next_action"].clone();
    assert!(action["arguments"]["continuation"].is_string(), "{wake}");
    assert_eq!(
        send(dir.path(), allowed.clone(), &action, shared)
            .await
            .status(),
        StatusCode::FORBIDDEN,
        "missing route scope grant"
    );
    let mut allowed = allowed;
    allowed.scope_ids.insert("route".into());
    // all_abouts grants select across abouts without making the handle itself a grant.
    let mut no_all = allowed.clone();
    no_all.scopes.remove("kmp:all-abouts");
    assert_eq!(
        send(dir.path(), no_all, &action, shared).await.status(),
        StatusCode::FORBIDDEN
    );
    let accepted = send(dir.path(), allowed, &action, shared).await;
    assert_eq!(accepted.status(), StatusCode::OK);
    assert_eq!(response_json(accepted).await["result"]["isError"], false);
}
