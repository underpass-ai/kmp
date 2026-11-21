use serde_json::{Value, json};

/// What a tool does to its environment, as its MCP annotations state it.
///
/// `Read` leaves memory byte-for-byte as it was (`tests/read_only_tools.rs`):
/// what such a call may leave behind is the server log, a bounded quality
/// telemetry row, a continuation handle when the answer has more pages and,
/// on a store whose owner opted into TypeSafe Jev, the cached verdicts —
/// none of which any tool reads back as memory. `open_world` is true where a
/// store that opted into Jev sends the text it reads to TypeSafe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolEffect {
    Read {
        open_world: bool,
    },
    Write {
        /// It may replace current state: an entry, a card, a label set.
        destructive: bool,
        /// Repeating the same call adds nothing more.
        idempotent: bool,
        open_world: bool,
    },
}

impl ToolEffect {
    pub(crate) fn annotations(self) -> Value {
        match self {
            Self::Read { open_world } => {
                json!({"readOnlyHint": true, "openWorldHint": open_world})
            }
            // A hint equal to its MCP default (destructive true, idempotent
            // false) says the same thing unwritten; readOnlyHint and
            // openWorldHint are always stated.
            Self::Write {
                destructive,
                idempotent,
                open_world,
            } => {
                let mut hints = json!({"readOnlyHint": false, "openWorldHint": open_world});
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
