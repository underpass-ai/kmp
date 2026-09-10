use super::*;

#[tokio::test]
async fn native_dependency_pages_share_returned_sources_and_keep_group_members_literal() {
    let dir = tempfile::tempdir().expect("isolated store");
    let mut server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let source =
        "S1 authorizes only local R19 execution; R20 and network access are excluded. ".repeat(30);
    let written = call(&server, "kmp_write_memory", json!({"about":"project:shared-dependencies",
        "actor":"test", "observed_at":"2026-09-01T10:00:00Z", "idempotency_key":"shared-dependencies:seed",
        "labels":{"source":["S1"]}, "memories":[
            {"id":"permit","kind":"constraint","summary":source,"evidence":source},
            {"id":"decision","kind":"decision","summary":"Execute R19 locally.",
             "observed_at":"2026-09-02T10:00:00Z", "evidence":"S2 explicitly chooses the operation allowed by S1.",
             "connect_to":[{"ref":"permit","rel":"chosen_because","class":"motivational","why":"S1's local-only conditions motivate the plan.","evidence":source}]}
        ]})).await;
    assert_eq!(written["isError"], false, "{written}");
    let mut args = json!({"about":"project:shared-dependencies", "at":{"time":"2026-09-03T00:00:00Z"},
        "axis":"observed", "include":{"dependencies":true},"limit":{"entries":1},
        "budget":{"max_bytes":100000}});
    let mut saw_shared_dependency = false;
    let mut saw_partial = false;
    for budget in [100000, 7000] {
        args["budget"]["max_bytes"] = json!(budget);
        args.as_object_mut().expect("arguments").remove("page");
        for index in 0..40 {
            server = server.with_shared_passages(false);
            let plain = call(&server, "kmp_goto", args.clone()).await;
            assert_eq!(plain["isError"], false, "{plain}");
            server = server.with_shared_passages(true);
            let shared = call(&server, "kmp_goto", args.clone()).await;
            assert_eq!(shared["isError"], false, "{shared}");
            let body = &shared["structuredContent"];
            assert_eq!(
                expand_packet(body.clone()).expect("page-local reconstruction"),
                plain["structuredContent"]
            );
            assert_eq!(
                body["proof"]["groups"],
                plain["structuredContent"]["proof"]["groups"]
            );
            saw_shared_dependency |= body["proof"]["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|entry| entry["text"].get("passage").is_some());
            if body["page"]["has_more"] == false {
                break;
            }
            saw_partial = true;
            assert!(index < 39, "continuation must finish");
            args = body["next_actions"][0]["arguments"].clone();
        }
    }
    assert!(saw_shared_dependency && saw_partial);
}
