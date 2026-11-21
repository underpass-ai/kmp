use serde_json::{Value, json};

/// What a tool does to its environment, as its MCP annotations state it.
///
/// `Read` leaves memory byte-for-byte as it was (`tests/read_only_tools.rs`):
/// what such a call may leave behind is the server log, a bounded quality
/// telemetry row, a continuation handle when the answer has more pages and,
/// on a store whose owner opted into TypeSafe Jev, the cached verdicts —
/// none of which any tool reads back as memory. Every tool is declared
/// closed to the outside world (`openWorldHint: false`); a session whose
/// store opted into Jev turns the hint on for the tools that then send text
/// to TypeSafe (`registry::with_open_world`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolEffect {
    Read,
    Write {
        /// It may replace current state: an entry, a card, a label set.
        destructive: bool,
        /// Repeating the same call adds nothing more.
        idempotent: bool,
    },
}

impl ToolEffect {
    pub(crate) fn annotations(self) -> Value {
        match self {
            Self::Read => json!({"readOnlyHint": true, "openWorldHint": false}),
            // A hint equal to its MCP default (destructive true, idempotent
            // false) says the same thing unwritten; readOnlyHint and
            // openWorldHint are always stated.
            Self::Write {
                destructive,
                idempotent,
            } => {
                let mut hints = json!({"readOnlyHint": false, "openWorldHint": false});
                if !destructive {
                    hints["destructiveHint"] = json!(false);
                }
                if idempotent {
                    hints["idempotentHint"] = json!(true);
                }
                hints
            }
        }
    }
}

pub(crate) fn tool_definition_with_output(
    name: &str,
    effect: ToolEffect,
    description: &str,
    input_schema: Value,
    output_schema: Value,
) -> Value {
    let mut definition = json!({
        "name": name,
        "description": description,
        "annotations": effect.annotations(),
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
