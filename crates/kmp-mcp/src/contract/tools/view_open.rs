use serde_json::{Value, json};

use crate::contract::schema::definition::{ToolEffect, tool_definition_with_output};
#[allow(unused_imports)]
use crate::contract::schema::primitives::*;
#[allow(unused_imports)]
use crate::contract::schema::relation_vocabulary::*;
#[allow(unused_imports)]
use crate::contract::schema::request_shape::*;
#[allow(unused_imports)]
use crate::contract::schema::response_shape::*;
use crate::contract::schema::view_family::view_output_with_url_schema;
pub(crate) fn definition() -> Value {
    tool_definition_with_output(
        "kmp_view_open",
        ToolEffect::Write {
            destructive: false,
            idempotent: false,
        },
        "Open a ChronoLoom view over an about so a human sees the memory you work on, with its link when the viewer is mounted.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["about"],
            "properties": {
                "about": string_schema("Memory anchor the loom should weave. It must exist: a view onto absent memory would render an empty loom that looks like an answer, so this is the one view call that fails on absence instead of degrading. Once a view is open, kmp_view_apply_intent drops what it cannot honor and names it in `unhonored`."),
                "view_id": string_schema("Which view to open. Omit for the one window a local viewer shows."),
                "expected_revision": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "When changing an existing view to another about, the view_revision this open was prepared against. A stale open conflicts instead of discarding a newer camera state."
                }
            }
        }),
        view_output_with_url_schema(),
    )
}
