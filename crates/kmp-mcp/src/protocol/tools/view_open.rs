//! `kmp_view_open` — put a human and this agent on the same loom.

use serde_json::{Value, json};

use crate::protocol::schema::string_schema;

pub(in crate::protocol) fn definition() -> Value {
    super::definition_with_output(
        "kmp_view_open",
        "Open or rehydrate a ChronoLoom view over an about, so a human and this agent look at the same loom. Returns this session's capability link when the local viewer is mounted. Read-only with respect to memory: a view is a camera position, not a record.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about"],
            "properties": {
                "about": string_schema("Memory anchor the loom should weave. It must exist; a view onto absent memory would render an empty loom that looks like an answer."),
                "view_id": string_schema("Which view to open. Omit for the one window a local viewer shows."),
                "expected_revision": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "When changing an existing view to another about, the view_revision this open was prepared against. A stale open conflicts instead of discarding a newer camera state."
                }
            }
        }),
        super::view_output::schema_with_url(),
    )
}
