use serde_json::{Value, json};

use crate::contract::schema::definition::tool_definition_with_output;
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
        "kmp_view_get_state",
        "Read the view's semantic state — clock, window, focus, zoom, filters, selection, revision and who last moved it. Returns this session's capability link when the local viewer is mounted; state, never pixels.",
        json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "view_id": string_schema("Which view to read. Omit for the default one.")
            }
        }),
        view_output_with_url_schema(),
    )
}
