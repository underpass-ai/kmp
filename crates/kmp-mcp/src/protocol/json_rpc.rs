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
