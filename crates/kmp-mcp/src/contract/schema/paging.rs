use serde_json::{Value, json};

use crate::contract::schema::primitives::*;
pub(crate) fn page_schema(entries_description: &str) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "entries": {
                "type": "integer",
                "minimum": 1,
                "description": entries_description
            },
            "cursor": string_schema("Opaque cursor returned by page.next_cursor.")
        }
    })
}
