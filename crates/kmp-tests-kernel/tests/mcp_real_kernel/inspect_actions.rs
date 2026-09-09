//! Continue, negotiate a budget and restart Inspect through each native transport.
use super::*;

pub(super) async fn check(
    direct: &GrpcKernelMcpBackend,
    stdio: &KernelMcpServer,
    http: &Router,
    embedded: &KernelMcpServer,
) {
    let mut args = json!({"about":"project:parity-live",
        "ref":"project:parity-live:observation:parity-after",
        "include":{"details":true,"incoming":true,"outgoing":true,"raw":true},
        "budget":{"max_bytes":512}});
    let first = same_success(direct, stdio, http, embedded, args.clone()).await;
    assert_eq!(first["structuredContent"]["page"]["returned"], 0);
    let mut page = first.clone();
    let mut completed = false;
    for _ in 0..50 {
        let content = &page["structuredContent"];
        if content["page"]["has_more"] == false {
            completed = true;
            break;
        }
        let action = &content["next_actions"][0];
        let stalled = content["page"]["returned"] == 0;
        assert_eq!(action["tool"], "kmp_inspect");
        args = action["arguments"].clone();
        args["page"]["repeat_object"] = json!(false);
        page = same_success(direct, stdio, http, embedded, args.clone()).await;
        assert_eq!(page["structuredContent"]["object_reused"], true);
        if stalled {
            assert!(
                page["structuredContent"]["page"]["returned"]
                    .as_u64()
                    .expect("valid inspection response")
                    > 0
            );
        }
    }
    assert!(completed);

    let mut changed = first["structuredContent"]["next_actions"][0]["arguments"].clone();
    changed["include"]["incoming"] = json!(false);
    changed["page"]["repeat_object"] = json!(false);
    assert_error_code_parity(
        direct,
        stdio,
        http,
        embedded,
        "kmp_inspect",
        changed.clone(),
        "conflict",
    )
    .await;
    let error = direct
        .call_tool("kmp_inspect", &changed)
        .await
        .expect_err("changed selection");
    let restart = &error.feedback[0]["action"];
    assert_eq!(restart["tool"], "kmp_inspect");
    assert!(restart["arguments"].get("page").is_none());
    let fresh = same_success(direct, stdio, http, embedded, restart["arguments"].clone()).await;
    assert!(fresh["structuredContent"].get("object_reused").is_none());
}

async fn same_success(
    direct: &GrpcKernelMcpBackend,
    stdio: &KernelMcpServer,
    http: &Router,
    embedded: &KernelMcpServer,
    arguments: Value,
) -> Value {
    let result = direct
        .call_tool("kmp_inspect", &arguments)
        .await
        .expect("direct Inspect");
    for actual in [
        call_tool(stdio, 900, "kmp_inspect", arguments.clone()).await,
        call_http_tool(http, 900, "kmp_inspect", arguments.clone()).await,
        call_tool(embedded, 900, "kmp_inspect", arguments).await,
    ] {
        assert_eq!(actual["result"], result, "Inspect action transport parity");
    }
    result
}
