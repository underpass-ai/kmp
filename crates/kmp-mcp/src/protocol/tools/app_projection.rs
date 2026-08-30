//! `kmp_view_read_projection` — one bounded ChronoLoom data chunk.
//!
//! One concept: the app transport's read. It is advertised only to the
//! sandboxed MCP App, and its structured payload is not a model prompt, so it
//! carries neither an output schema nor model visibility.

use serde_json::{Value, json};

use crate::protocol::chronoloom_app::CHRONOLOOM_APP_URI;

use crate::protocol::request_shape::dimensions_schema;
use crate::protocol::schema::string_schema;

pub(in crate::protocol) fn input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["about", "from", "to"],
        "properties": {
            "about": string_schema("Memory anchor to project."),
            "from": string_schema("Inclusive RFC3339 range start."),
            "to": string_schema("Exclusive RFC3339 range end."),
            "axis": {
                "type": "string",
                "enum": ["occurred", "observed", "ingested", "validity"]
            },
            "lod": {
                "type": "string",
                "enum": ["atlas", "episode", "moment"]
            },
            "bins": {"type": "integer", "minimum": 1, "maximum": 512},
            "limit": {"type": "integer", "minimum": 1, "maximum": 2048},
            "cursor": string_schema("Opaque continuation cursor returned by the prior chunk."),
            "depth": {"type": "integer", "minimum": 1, "maximum": 6},
            "dimensions": dimensions_schema()
        }
    })
}

pub(in crate::protocol) fn definition() -> Value {
    json!({
        "name": "kmp_view_read_projection",
        "description": "Read one bounded, paginated ChronoLoom data chunk. This tool is available to the sandboxed MCP App only; its structured payload is not a model prompt.",
        "inputSchema": input_schema(),
        "annotations": {
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
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
