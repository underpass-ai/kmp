//! Review the explicitly authored writes used to set up unrelated read tests.
//! Protocol tests for needs_review use raw calls and never this fixture helper.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

pub async fn review_authored_write(server: &KernelMcpServer, result: Value) -> Value {
    let envelope = result.get("structuredContent").is_some();
    let body = result.get("structuredContent").unwrap_or(&result);
    if body["status"] != "needs_review" {
        return result;
    }
    assert_eq!(
        body["accepted"], false,
        "pending fixtures must apply nothing"
    );
    let action = &body["next_actions"][0];
    assert_eq!(action["tool"], "kmp_write_memory");
    let request = json!({"jsonrpc":"2.0","id":99,"method":"tools/call","params":{"name":action["tool"],"arguments":action["arguments"]}});
    let reply = server
        .handle_json_line(&request.to_string())
        .await
        .expect("fixture review");
    let result = serde_json::from_str::<Value>(&reply).expect("JSON")["result"].clone();
    assert_eq!(result["isError"], false, "{result}");
    assert!(
        matches!(
            result["structuredContent"]["status"].as_str(),
            Some("validated" | "committed" | "replayed")
        ),
        "{result}"
    );
    if envelope {
        result
    } else {
        result["structuredContent"].clone()
    }
}
