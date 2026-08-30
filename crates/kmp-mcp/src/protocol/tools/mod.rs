//! One advertised tool per module, plus the builder they share.
//!
//! A tool's module owns what is true of that tool alone: its description, its
//! input schema, and the shape of what it answers. A shape two or more tools
//! share belongs in `schema`, `request_shape` or `response_shape` instead —
//! and a shape only one tool uses belongs here, beside it, however large.

pub(in crate::protocol) mod app_projection;
pub(in crate::protocol) mod app_view_undo;
pub(in crate::protocol) mod ask;
pub(in crate::protocol) mod ingest;
pub(in crate::protocol) mod inspect;
pub(in crate::protocol) mod temporal;
pub(in crate::protocol) mod trace;
pub(in crate::protocol) mod view_apply_intent;
pub(in crate::protocol) mod view_get_state;
pub(in crate::protocol) mod view_open;
pub(in crate::protocol) mod view_output;
pub(in crate::protocol) mod wake;
pub(in crate::protocol) mod write_memory;

use serde_json::{Value, json};

/// A tool, with the shape of what it answers.
///
/// Inputs were described field by field and the response — the half the agent
/// actually reasons over — was described nowhere. `proof.confidence`,
/// `proof.superseded` and `proof.expired` against `proof.conflicts`, `page.total`,
/// `projection.next_action`, `resume_cursor`: every one of them arrived
/// unexplained, and what did explain them was `SKILL.md`, a Claude Code plugin
/// file that an agent in any other host never sees.
///
/// A memory kernel whose contract is only legible inside one vendor's plugin
/// is not a protocol.
pub(in crate::protocol) fn definition_with_output(
    name: &str,
    description: &str,
    input_schema: Value,
    output_schema: Value,
) -> Value {
    let mut definition = json!({
        "name": name,
        "description": description,
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
