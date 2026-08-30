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
pub(crate) fn input_schema() -> Value {
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

pub(crate) fn definition() -> Value {
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
