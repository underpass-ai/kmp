use serde_json::Value;

use crate::contract::registry::tools_list_result;
use crate::contract::tools::{app_view_undo, app_visual_projection};
use crate::serving::ToolError;

pub(crate) fn reject_unknown_arguments(tool: &str, arguments: &Value) -> Result<(), ToolError> {
    let Some(schema) = tool_input_schema(tool) else {
        return Ok(());
    };
    check_against_schema(schema, arguments, tool)
}

/// The schemas, built once.
///
/// This runs on every tool call, and `tools_list_result()` builds the whole
/// full tool document — relation vocabulary included — from scratch each time.
/// Rebuilding a document that cannot change, per call, to read one field of
/// it, is a cost with nothing on the other side of it.
fn tool_input_schema(tool: &str) -> Option<&'static Value> {
    if tool == "kmp_view_read_projection" {
        static APP_VISUAL_SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        return Some(APP_VISUAL_SCHEMA.get_or_init(app_visual_projection::input_schema));
    }
    if tool == "kmp_view_undo" {
        static APP_UNDO_SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        return Some(
            APP_UNDO_SCHEMA.get_or_init(|| app_view_undo::definition()["inputSchema"].clone()),
        );
    }
    static SCHEMAS: std::sync::OnceLock<std::collections::BTreeMap<String, Value>> =
        std::sync::OnceLock::new();
    SCHEMAS
        .get_or_init(|| {
            tools_list_result()["tools"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|definition| {
                    Some((
                        definition["name"].as_str()?.to_string(),
                        definition["inputSchema"].clone(),
                    ))
                })
                .collect()
        })
        .get(tool)
}

fn check_against_schema(schema: &Value, value: &Value, path: &str) -> Result<(), ToolError> {
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

pub(crate) fn validate_required_arguments(
    arguments: &Value,
    required_arguments: &[&str],
) -> Result<(), String> {
    let Some(arguments) = arguments.as_object() else {
        return Err("tool arguments must be a JSON object".to_string());
    };

    for required_argument in required_arguments {
        let present = arguments
            .get(*required_argument)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());

        if !present {
            return Err(format!("missing required argument `{required_argument}`"));
        }
    }

    Ok(())
}

pub(crate) fn required_string(arguments: &Value, key: &str) -> Result<String, String> {
    arguments
        .as_object()
        .and_then(|arguments| arguments.get(key))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing required argument `{key}`"))
}

pub(crate) fn optional_string(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .as_object()
        .and_then(|arguments| arguments.get(key))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn validates_non_empty_required_string_arguments() {
        let arguments = json!({
            "about": "node:root",
            "question": "What changed?"
        });

        assert!(validate_required_arguments(&arguments, &["about", "question"]).is_ok());
        assert_eq!(
            required_string(&arguments, "about").expect("valid about argument should be accepted"),
            "node:root"
        );
    }

    #[test]
    fn rejects_missing_blank_or_non_object_required_arguments() {
        assert_eq!(
            validate_required_arguments(&Value::Null, &["about"])
                .expect_err("non-object arguments should be rejected"),
            "tool arguments must be a JSON object"
        );
        assert_eq!(
            validate_required_arguments(&json!({"about": "  "}), &["about"])
                .expect_err("blank about should be rejected"),
            "missing required argument `about`"
        );
        assert_eq!(
            required_string(&json!({}), "about").expect_err("missing about should be rejected"),
            "missing required argument `about`"
        );
    }

    #[test]
    fn reads_optional_strings() {
        let arguments = json!({
            "role": "reader"
        });

        assert_eq!(
            optional_string(&arguments, "role").as_deref(),
            Some("reader")
        );
        assert_eq!(optional_string(&json!({"role": ""}), "role"), None);
    }
}
