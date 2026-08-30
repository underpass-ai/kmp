//! One concept: holding a call to the input schema its tool advertises.
//!
//! Nothing else in `protocol` reads arguments. This module reads them and
//! nothing else, which is why the check can be recursive over any schema
//! shape without knowing a single tool by name.

use serde_json::Value;

use crate::protocol::registry::tool_input_schema;
use crate::tool_error::ToolError;

/// Applies the strictness the schemas already declare.
///
/// Every model-facing tool says `"additionalProperties": false` and nothing
/// enforced it, which made the surface a silent-failure generator: a
/// misspelled `dimensions`, a `budget` nested one level too deep, a `from`
/// sent to `kmp_goto` where the cursor is `at` — each accepted, dropped,
/// and answered with a well-formed success built from defaults. The agent has
/// no way to tell a request that was honoured from one that was discarded, so
/// it reads the result as proof its arguments were understood and makes the
/// same call again.
///
/// The check reads the published schema rather than a second list, so it
/// cannot drift from what `tools/list` promises.
pub(crate) fn reject_unknown_arguments(tool: &str, arguments: &Value) -> Result<(), ToolError> {
    let Some(schema) = tool_input_schema(tool) else {
        return Ok(());
    };
    check_against_schema(schema, arguments, tool)
}

pub(in crate::protocol) fn check_against_schema(
    schema: &Value,
    value: &Value,
    path: &str,
) -> Result<(), ToolError> {
    let (Some(properties), Some(object)) = (schema["properties"].as_object(), value.as_object())
    else {
        return Ok(());
    };

    if schema["additionalProperties"] == Value::Bool(false) {
        for key in object.keys() {
            if properties.contains_key(key) {
                continue;
            }
            let known = properties.keys().cloned().collect::<Vec<_>>().join(", ");
            return Err(ToolError::invalid_argument(format!(
                "`{path}` has no argument `{key}`. This call would otherwise have been answered \
                 with that argument silently dropped. Accepted here: {known}."
            )));
        }
    }

    for (key, nested) in object {
        let Some(nested_schema) = properties.get(key) else {
            continue;
        };
        let nested_path = format!("{path}.{key}");
        check_against_schema(nested_schema, nested, &nested_path)?;
        if let (Some(items), Some(array)) = (nested_schema.get("items"), nested.as_array()) {
            for entry in array {
                check_against_schema(items, entry, &nested_path)?;
            }
        }
    }
    Ok(())
}
