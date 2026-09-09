//! Exercise returned continuation and recovery calls through all four transports.
use super::*;

pub(super) async fn check(
    direct: &GrpcKernelMcpBackend,
    stdio: &KernelMcpServer,
    http: &Router,
    embedded: &KernelMcpServer,
) {
    let mut stale = Value::Null;
    for (tool, mut arguments) in [
        (
            "kmp_relate",
            json!({"about":"project:parity-live","page":{"entries":1},"budget":{"max_bytes":100000}}),
        ),
        (
            "kmp_trace",
            json!({"about":"project:parity-live","from":"project:parity-live:observation:parity-after",
            "to":"project:parity-live:observation:parity-before","budget":{"max_bytes":512}}),
        ),
    ] {
        let mut done = false;
        for _ in 0..20 {
            let result = same_success(direct, stdio, http, embedded, tool, arguments).await;
            let content = &result["structuredContent"];
            if content["page"]["has_more"] == false {
                done = true;
                break;
            }
            let action = &content["next_actions"][0];
            assert_eq!(action["tool"], tool);
            if tool == "kmp_relate" && stale.is_null() {
                stale = action["arguments"].clone();
            }
            arguments = action["arguments"].clone();
        }
        assert!(done, "returned actions must complete {tool}");
    }
    assert!(
        !stale.is_null(),
        "fixture must exercise multiple Relate pages"
    );
    let mut mutation = parity_seed_arguments();
    mutation["idempotency_key"] = json!("parity:cursor-change");
    mutation["memory"]["entries"][0]["metadata"] = json!({"cursor_revision":"changed"});
    direct
        .call_tool("kmp_ingest", &mutation)
        .await
        .expect("remote mutation");
    assert_tool_success(&call_tool(embedded, 210, "kmp_ingest", mutation).await);
    assert_error_code_parity(
        direct,
        stdio,
        http,
        embedded,
        "kmp_relate",
        stale.clone(),
        "conflict",
    )
    .await;
    let error = direct
        .call_tool("kmp_relate", &stale)
        .await
        .expect_err("stale cursor");
    let restart = &error.feedback[0]["action"];
    assert!(restart["arguments"].pointer("/page/cursor").is_none());
    same_success(
        direct,
        stdio,
        http,
        embedded,
        "kmp_relate",
        restart["arguments"].clone(),
    )
    .await;
}

async fn same_success(
    direct: &GrpcKernelMcpBackend,
    stdio: &KernelMcpServer,
    http: &Router,
    embedded: &KernelMcpServer,
    tool: &str,
    arguments: Value,
) -> Value {
    let result = direct
        .call_tool(tool, &arguments)
        .await
        .expect("direct read");
    for actual in [
        call_tool(stdio, 200, tool, arguments.clone()).await,
        call_http_tool(http, 200, tool, arguments.clone()).await,
        call_tool(embedded, 200, tool, arguments).await,
    ] {
        assert_eq!(actual["result"], result, "{tool} transport parity");
    }
    result
}
