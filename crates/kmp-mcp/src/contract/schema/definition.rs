use serde_json::{Value, json};

pub(crate) fn tool_definition_with_output(
    name: &str,
    destructive: bool,
    description: &str,
    input_schema: Value,
    output_schema: Value,
) -> Value {
    let mut definition = json!({
        "name": name,
        "description": description,
        // Even retrieval calls may persist guidance usage or read diagnostics.
        // The destructive flag describes replacement of current state, not
        // deletion of the canonical event history.
        "annotations": {
            "readOnlyHint": false,
            "openWorldHint": false,
            "destructiveHint": destructive
        },
        "inputSchema": input_schema,
        "_meta": {
            "anthropic/maxResultSizeChars": 10_000
        }
    });
    if !output_schema.is_null() {
        definition["outputSchema"] = output_schema;
    }
    definition
}
