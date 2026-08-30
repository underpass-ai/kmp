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
        "name": "kmp_view_undo",
        "description": "Undo the latest semantic view move. This is an app-only transport adapter over the same reversible view aggregate used by the loopback renderer.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "view_id": string_schema("The app view to undo. Omit for the default view.")
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
