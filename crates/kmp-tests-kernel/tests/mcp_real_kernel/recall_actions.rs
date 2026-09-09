//! Returned recall calls and recovery survive every transport without rewriting.
use super::*;

pub(super) async fn check(
    direct: &GrpcKernelMcpBackend,
    stdio: &KernelMcpServer,
    http: &Router,
    embedded: &KernelMcpServer,
) {
    for tool in ["kmp_wake", "kmp_ask"] {
        let mut args = json!({"about":"project:parity-live","axis":"occurred",
            "interval":{"start":"2026-08-01T00:00:00Z"},
            "budget":{"max_bytes":512,"detail":"full","depth":5}});
        if tool == "kmp_ask" {
            args["question"] = json!("Did the compact projection stay under the byte limit?");
            args["asked_as"] = json!("¿La proyección compacta respetó el límite de bytes?");
        }
        let first = same(direct, stdio, http, embedded, tool, args).await;
        let mut page = first.clone();
        let mut complete = false;
        for _ in 0..100 {
            let content = &page["structuredContent"];
            if content["projection"]["next_action"].is_null() {
                complete = true;
                break;
            }
            let action = &content["projection"]["next_action"];
            assert_eq!(action["tool"], tool);
            let stalled = content["projection"]["page"]["returned"] == 0;
            page = same(
                direct,
                stdio,
                http,
                embedded,
                tool,
                action["arguments"].clone(),
            )
            .await;
            if stalled {
                assert!(
                    page["structuredContent"]["projection"]["page"]["returned"]
                        .as_u64()
                        .expect("progress")
                        > 0
                );
            }
        }
        assert!(complete);
        let mut paged_args =
            first["structuredContent"]["projection"]["next_action"]["arguments"].clone();
        paged_args["page"]["entries"] = json!(1);
        let paged = same(direct, stdio, http, embedded, tool, paged_args).await;
        let mut changed =
            paged["structuredContent"]["projection"]["next_action"]["arguments"].clone();
        assert!(
            changed["page"]["cursor"].is_string(),
            "expected continuation: {paged}"
        );
        changed["axis"] = json!("ingested");
        let error = same(direct, stdio, http, embedded, tool, changed).await;
        assert_eq!(error["isError"], true, "{tool}: {error}");
        assert_eq!(error["structuredContent"]["error"]["code"], "conflict");
        let restart = &error["structuredContent"]["feedback"][0]["action"];
        assert_eq!(restart["tool"], tool);
        assert!(restart["arguments"]["page"].get("cursor").is_none());
        let restarted = same(
            direct,
            stdio,
            http,
            embedded,
            tool,
            restart["arguments"].clone(),
        )
        .await;
        assert_ne!(restarted["isError"], true);
    }
}

async fn same(
    direct: &GrpcKernelMcpBackend,
    stdio: &KernelMcpServer,
    http: &Router,
    embedded: &KernelMcpServer,
    tool: &str,
    arguments: Value,
) -> Value {
    let result = match direct.call_tool(tool, &arguments).await {
        Ok(result) => result,
        Err(error) => {
            // The public server serializes the same typed error as the others.
            let reply = call_tool(stdio, 990, tool, arguments.clone()).await;
            let result = reply["result"].clone();
            assert_eq!(
                result["structuredContent"]["error"]["code"],
                error.code.as_str()
            );
            assert_eq!(
                result["structuredContent"]["feedback"],
                json!(error.feedback)
            );
            result
        }
    };
    for actual in [
        call_tool(stdio, 991, tool, arguments.clone()).await,
        call_http_tool(http, 991, tool, arguments.clone()).await,
        call_tool(embedded, 991, tool, arguments).await,
    ] {
        assert_eq!(actual["result"], result, "{tool} action parity");
    }
    result
}
