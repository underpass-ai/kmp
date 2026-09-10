//! Wire/projection coverage; traversal behavior is checked by the real-service tests.
use super::*;

#[tokio::test]
async fn all_four_temporal_tools_transport_dependency_records_and_groups() {
    let recorded = RecordedMemoryRequests::default();
    let endpoint = spawn_fake_memory_server(recorded.clone()).await;
    let server = KernelMcpServer::grpc(endpoint);
    for (tool, cursor) in [
        ("kmp_goto", "at"),
        ("kmp_near", "around"),
        ("kmp_rewind", "from"),
        ("kmp_forward", "from"),
    ] {
        let mut args = json!({"about":"question:temporal", "include":{"dependencies":true},
            "refs":["claim:rachel-austin"],
            "budget":{"max_bytes":100000}});
        args[cursor] = json!({"time":"2026-04-12T15:03:00Z"});
        let result = call_tool(&server, 20, tool, args).await;
        assert_eq!(result["result"]["isError"], false, "{result}");
        let content = &result["result"]["structuredContent"];
        assert_eq!(
            content["proof"]["groups"][0]["member_refs"],
            json!(["claim:rachel-austin", "claim:rachel-denver"])
        );
        assert_eq!(
            content["proof"]["entries"][0]["text"],
            "Rachel said she was moving to Denver."
        );
        assert!(
            !content["proof"]["entries"][0]["coordinates"]
                .as_array()
                .expect("expected dependency wire fixture")
                .is_empty()
        );
    }
    assert!(recorded.moves().await.iter().all(|value| {
        assert_eq!(
            value
                .request
                .entry_selection
                .as_ref()
                .expect("entry focus")
                .refs,
            ["claim:rachel-austin"]
        );
        value
            .request
            .include
            .as_ref()
            .expect("expected dependency wire fixture")
            .dependencies
    }));
    assert!(recorded.nears().await.iter().all(|value| {
        assert_eq!(
            value.entry_selection.as_ref().expect("entry focus").refs,
            ["claim:rachel-austin"]
        );
        value
            .include
            .as_ref()
            .expect("expected dependency wire fixture")
            .dependencies
    }));
}
