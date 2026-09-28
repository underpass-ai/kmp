use serde_json::Value;

use crate::contract::registry::tools_list_result;
use crate::contract::tools::{app_view_take_control, app_view_undo, app_visual_projection};
use crate::serving::ToolError;

/// Refuses an argument the tool does not declare. Structural only: a writer
/// that owns field-level refusals runs this before its own planner.
pub(crate) fn reject_unknown_arguments(tool: &str, arguments: &Value) -> Result<(), ToolError> {
    let Some(schema) = tool_input_schema(tool) else {
        return Ok(());
    };
    check_against_schema(schema, arguments, tool, Lexical::Skip)
}

/// Unknown arguments plus the lexical constraints (`minLength`,
/// `uniqueItems`) the advertised catalogue omits (#850). A tool whose planner
/// gives a more specific refusal runs this after that planner, as a backstop.
pub(crate) fn reject_invalid_arguments(tool: &str, arguments: &Value) -> Result<(), ToolError> {
    let Some(schema) = tool_input_schema(tool) else {
        return Ok(());
    };
    check_against_schema(schema, arguments, tool, Lexical::Enforce)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lexical {
    Skip,
    Enforce,
}

/// The schemas, built once.
///
/// This runs on every tool call, and `tools_list_result()` builds the whole
/// full tool document — relation vocabulary included — from scratch each time.
/// Rebuilding a document that cannot change, per call, to read one field of
/// it, is a cost with nothing on the other side of it.
fn tool_input_schema(tool: &str) -> Option<&'static Value> {
    if tool == "kmp_view_read_nodes" {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        return Some(SCHEMA.get_or_init(|| {
            crate::contract::tools::app_memory_nodes::definition()["inputSchema"].clone()
        }));
    }
    if tool == "kmp_view_read_projection" {
        static APP_VISUAL_SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        return Some(APP_VISUAL_SCHEMA.get_or_init(app_visual_projection::input_schema));
    }
    if tool == "kmp_view_take_control" {
        static SCHEMA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        return Some(
            SCHEMA.get_or_init(|| app_view_take_control::definition()["inputSchema"].clone()),
        );
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

/// Walks an argument value beside its schema. Beyond unknown arguments it
/// enforces the two lexical constraints the advertised catalogue leaves out to
/// save tokens (#850): `minLength` and `uniqueItems` stay in this internal
/// contract, so dropping them from `tools/list` never widens what is accepted.
fn check_against_schema(
    schema: &Value,
    value: &Value,
    path: &str,
    lexical: Lexical,
) -> Result<(), ToolError> {
    let schema = union_branch(schema, value).unwrap_or(schema);
    match value {
        Value::String(text) if lexical == Lexical::Enforce => check_length(schema, text, path),
        Value::Array(array) => {
            if lexical == Lexical::Enforce {
                check_unique(schema, array, path)?;
            }
            if let Some(items) = schema.get("items") {
                for (index, entry) in array.iter().enumerate() {
                    check_against_schema(items, entry, &format!("{path}[{index}]"), lexical)?;
                }
            }
            Ok(())
        }
        Value::Object(object) => check_object(schema, object, path, lexical),
        _ => Ok(()),
    }
}

fn check_object(
    schema: &Value,
    object: &serde_json::Map<String, Value>,
    path: &str,
    lexical: Lexical,
) -> Result<(), ToolError> {
    let properties = schema["properties"].as_object();
    if let Some(properties) = properties
        && schema["additionalProperties"] == Value::Bool(false)
    {
        for key in object.keys() {
            if properties.contains_key(key) {
                continue;
            }
            let known = properties.keys().cloned().collect::<Vec<_>>().join(", ");
            let error = ToolError::invalid_argument(format!(
                "`{path}` has no argument `{key}`. This call would otherwise have been answered \
                 with that argument silently dropped. Accepted here: {known}."
            ));
            return Err(if path.starts_with("kmp_write_memory") {
                error.with_feedback(serde_json::json!({
                    "code": "UNKNOWN_ARGUMENT", "severity": "error",
                    "field": format!("{}.{}", path.strip_prefix("kmp_write_memory.").unwrap_or_default(), key).trim_start_matches('.'),
                    "reason": "This field is not part of the native writer contract; remove it or use a declared field.",
                    "action": null
                }))
            } else {
                error
            });
        }
    }

    let additional = schema
        .get("additionalProperties")
        .filter(|additional| additional.is_object());
    for (key, nested) in object {
        let nested_schema = properties.and_then(|properties| properties.get(key));
        if let Some(nested_schema) = nested_schema.or(additional) {
            check_against_schema(nested_schema, nested, &format!("{path}.{key}"), lexical)?;
        }
    }
    Ok(())
}

/// The `oneOf`/`anyOf` branch declared for this value's JSON type, if any.
/// Constraints live on the branch that can hold the value; a branch without a
/// `type` only states requirements, which are checked elsewhere.
fn union_branch<'a>(schema: &'a Value, value: &Value) -> Option<&'a Value> {
    let kind = match value {
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::Null => "null",
    };
    ["oneOf", "anyOf"]
        .into_iter()
        .filter_map(|key| schema.get(key)?.as_array())
        .flatten()
        .find(|branch| match &branch["type"] {
            Value::String(declared) => {
                declared == kind || (declared == "number" && kind == "integer")
            }
            Value::Array(declared) => declared
                .iter()
                .any(|declared| declared == kind || (declared == "number" && kind == "integer")),
            _ => false,
        })
}

fn check_length(schema: &Value, text: &str, path: &str) -> Result<(), ToolError> {
    match schema.get("minLength").and_then(Value::as_u64) {
        Some(minimum) if (text.chars().count() as u64) < minimum => {
            Err(ToolError::invalid_argument(format!(
                "`{path}` must not be empty: give it a value or leave the argument out."
            )))
        }
        _ => Ok(()),
    }
}

fn check_unique(schema: &Value, array: &[Value], path: &str) -> Result<(), ToolError> {
    if schema.get("uniqueItems") != Some(&Value::Bool(true)) {
        return Ok(());
    }
    for (index, entry) in array.iter().enumerate() {
        if let Some(first) = array[..index].iter().position(|seen| seen == entry) {
            return Err(ToolError::invalid_argument(format!(
                "`{path}` lists the same value twice (items {first} and {index}): {entry}. \
                 Give each value once."
            )));
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
    fn an_empty_idempotency_key_is_refused_although_the_catalogue_omits_min_length() {
        assert!(
            reject_unknown_arguments(
                "kmp_ingest",
                &json!({"about": "question:x", "idempotency_key": ""})
            )
            .is_ok(),
            "the structural check alone leaves lexical refusals to the backstop"
        );
        let error = reject_invalid_arguments(
            "kmp_ingest",
            &json!({"about": "question:x", "idempotency_key": ""}),
        )
        .expect_err("an empty key must be refused");
        assert!(
            error
                .message
                .contains("`kmp_ingest.idempotency_key` must not be empty"),
            "{}",
            error.message
        );
    }

    #[test]
    fn empty_strings_are_refused_inside_arrays_maps_and_unions() {
        let schema = json!({"type":"object","additionalProperties":false,"properties":{
            "refs":{"type":"array","items":{"type":"string","minLength":1}},
            "labels":{"type":"object","additionalProperties":{"type":"array","uniqueItems":true,"items":{"type":"string","minLength":1}}},
            "steps":{"type":"array","items":{"oneOf":[{"type":"string","minLength":1},{"type":"object","properties":{"rel":{"type":"string","minLength":1}}}]}}
        }});
        let refused = |value: Value| {
            check_against_schema(&schema, &value, "t", Lexical::Enforce)
                .expect_err("refused")
                .message
        };

        assert!(refused(json!({"refs":["a",""]})).contains("`t.refs[1]` must not be empty"));
        assert!(refused(json!({"labels":{"topic":[""]}})).contains("`t.labels.topic[0]`"));
        assert!(
            refused(json!({"labels":{"topic":["a","a"]}}))
                .contains("same value twice (items 0 and 1)")
        );
        assert!(refused(json!({"steps":[""]})).contains("`t.steps[0]` must not be empty"));
        assert!(refused(json!({"steps":[{"rel":""}]})).contains("`t.steps[0].rel`"));
        assert!(
            check_against_schema(
                &schema,
                &json!({"refs":["a"],"labels":{"topic":["a","b"]},"steps":["x",{"rel":"y"}]}),
                "t",
                Lexical::Enforce
            )
            .is_ok()
        );
    }

    #[test]
    fn duplicate_items_are_refused_where_the_contract_declares_them_unique() {
        let schema = json!({"type":"array","uniqueItems":true,"items":{"type":"array"}});
        assert!(
            check_against_schema(
                &schema,
                &json!([["a", "b"], ["a", "b"]]),
                "t",
                Lexical::Enforce
            )
            .is_err()
        );
        assert!(
            check_against_schema(
                &schema,
                &json!([["a", "b"], ["b", "a"]]),
                "t",
                Lexical::Enforce
            )
            .is_ok()
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
