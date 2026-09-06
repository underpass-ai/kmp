use serde_json::{Value, json};

use crate::contract::handshake::CHRONOLOOM_APP_URI;

#[allow(unused_imports)]
use crate::contract::schema::primitives::*;
#[allow(unused_imports)]
use crate::contract::schema::relation_vocabulary::*;
#[allow(unused_imports)]
use crate::contract::schema::request_shape::*;
#[allow(unused_imports)]
use crate::contract::schema::response_shape::*;
pub(crate) fn definition() -> Value {
    json!({
        "name": "kmp_view_take_control",
        "description": "Take human control of the current frame without moving it. Refuses a stale revision. App-only adapter over the shared view aggregate.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["expected_revision"],
            "properties": {
                "view_id": string_schema("The shared view. Omit for the default view."),
                "expected_revision": {"type": "integer", "minimum": 1}
            }
        },
        "annotations": {
            "readOnlyHint": false,
            "destructiveHint": false,
            "idempotentHint": false,
            "openWorldHint": false
        },
        "_meta": {
            "ui": {
                "resourceUri": CHRONOLOOM_APP_URI,
                "visibility": ["app"]
            }
        }
    })
}
