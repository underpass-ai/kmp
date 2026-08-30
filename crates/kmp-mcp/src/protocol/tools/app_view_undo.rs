//! `kmp_view_undo` — the app's reversal of the latest semantic view move.
//!
//! One concept: an app-only transport adapter over the same reversible view
//! aggregate the loopback renderer uses.

use serde_json::{Value, json};

use crate::protocol::chronoloom_app::CHRONOLOOM_APP_URI;

use crate::protocol::schema::string_schema;

pub(in crate::protocol) fn definition() -> Value {
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
