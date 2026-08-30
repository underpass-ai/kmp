//! `kmp_view_get_state` — read the loom's semantic state, never its pixels.

use serde_json::{Value, json};

use crate::protocol::schema::string_schema;

pub(in crate::protocol) fn definition() -> Value {
    super::definition_with_output(
        "kmp_view_get_state",
        "Read the view's semantic state — clock, window, focus, zoom, filters, selection, revision and who last moved it. Returns this session's capability link when the local viewer is mounted; state, never pixels.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "view_id": string_schema("Which view to read. Omit for the default one.")
            }
        }),
        super::view_output::schema_with_url(),
    )
}
