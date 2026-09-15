//! JSON-RPC framing: how a result and an error are put on the wire.
//!
//! The outermost envelope, which knows nothing of KMP — not a tool, not a
//! schema, not a store. Separated so the transport shape can be read and
//! changed without walking through the tool catalog.

use serde_json::{Value, json};

pub(crate) fn jsonrpc_result(id: Value, result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
    .to_string()
}

pub(crate) fn jsonrpc_error(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jsonrpc_helpers_wrap_results_and_errors() {
        let result = serde_json::from_str::<Value>(&jsonrpc_result(json!(7), json!({"ok": true})))
            .expect("result should be JSON");
        assert_eq!(result["jsonrpc"], "2.0");
        assert_eq!(result["id"], 7);
        assert_eq!(result["result"]["ok"], true);

        let error = serde_json::from_str::<Value>(&jsonrpc_error(json!(8), -32601, "missing"))
            .expect("error should be JSON");
        assert_eq!(error["id"], 8);
        assert_eq!(error["error"]["code"], -32601);
        assert_eq!(error["error"]["message"], "missing");
    }
}
