use serde_json::Value;

use super::schema_law;
use super::schema_violation::SchemaViolation;
use crate::contract::registry::tools_list_result;
use crate::contract::tools::{app_view_take_control, app_view_undo, app_visual_projection};
use crate::serving::ToolError;

/// Refuses a call the tool's input schema refuses, before anything reads it.
///
/// The schema is the law of the call (#921, E1): type, enum, numeric and
/// array bounds, pattern, required arguments, unknown arguments and the
/// conditional shapes. `minLength` and `uniqueItems` are enforced although the
/// advertised catalogue leaves them out to save tokens (#850). A name this
/// surface does not declare has no schema, and is refused as an unknown tool.
pub(crate) fn reject_invalid_arguments(tool: &str, arguments: &Value) -> Result<(), ToolError> {
    let Some(schema) = tool_input_schema(tool) else {
        return Err(ToolError::unknown_tool(format!(
            "`{tool}` is not a tool on this surface; tools/list names the tools it has."
        )));
    };
    schema_law::check(schema, arguments, tool).map_err(|violation| refusal(tool, violation))
}

/// Every schema refusal carries one feedback item naming the field, so a
/// host can repair the call without parsing the sentence.
fn refusal(tool: &str, violation: SchemaViolation) -> ToolError {
    let error = ToolError::invalid_argument(violation.message.clone());
    let mut feedback = if violation.unknown {
        serde_json::json!({
            "code": "UNKNOWN_ARGUMENT", "severity": "error", "field": violation.field,
            "reason": if tool == "kmp_write_memory" {
                "This field is not part of the native writer contract; remove it or use a declared field.".to_string()
            } else {
                violation.message.clone()
            },
            "action": null
        })
    } else {
        serde_json::json!({
            "code": "SCHEMA_VIOLATION", "severity": "error", "field": violation.field,
            "reason": violation.message, "action": null
        })
    };
    if !violation.allowed.is_empty() {
        feedback["allowed"] = Value::Array(violation.allowed);
    }
    error.with_feedback(feedback)
}

/// The schemas, built once.
///
/// This runs on every tool call, and `tools_list_result()` builds the whole
/// full tool document — relation vocabulary included — from scratch each time.
/// Rebuilding a document that cannot change, per call, to read one field of
/// it, is a cost with nothing on the other side of it.
pub(crate) fn tool_input_schema(tool: &str) -> Option<&'static Value> {
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
            schema_law::check(&schema, &value, "t")
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
            schema_law::check(
                &schema,
                &json!({"refs":["a"],"labels":{"topic":["a","b"]},"steps":["x",{"rel":"y"}]}),
                "t",
            )
            .is_ok()
        );
    }

    #[test]
    fn duplicate_items_are_refused_where_the_contract_declares_them_unique() {
        let schema = json!({"type":"array","uniqueItems":true,"items":{"type":"array"}});
        assert!(schema_law::check(&schema, &json!([["a", "b"], ["a", "b"]]), "t").is_err());
        assert!(schema_law::check(&schema, &json!([["a", "b"], ["b", "a"]]), "t").is_ok());
    }

    #[test]
    fn a_name_the_surface_does_not_declare_is_refused_as_an_unknown_tool() {
        for name in [
            "kernel_remember",
            "kernel_ingest_context",
            "kernel_wake",
            "kmp_zz",
        ] {
            let error = reject_invalid_arguments(name, &json!({"about": "x"}))
                .expect_err("an undeclared name has no schema and is refused");
            assert_eq!(
                error.code,
                crate::serving::ToolErrorCode::UnknownTool,
                "{name}"
            );
        }
    }

    #[test]
    fn a_write_refusal_carries_field_feedback() {
        let error = reject_invalid_arguments(
            "kmp_write_memory",
            &json!({"about": "a:b", "actor": "t", "labels": {"t": ["x"]},
                    "memories": [{"id": "m", "kind": "observation", "summary": "S.",
                                  "evidence": "E.", "valid_until": 1798761600}]}),
        )
        .expect_err("a number for a timestamp is refused");
        assert_eq!(error.feedback[0]["code"], "SCHEMA_VIOLATION");
        assert_eq!(error.feedback[0]["field"], "memories[0].valid_until");
        let error = reject_invalid_arguments(
            "kmp_write_memory",
            &json!({"about": "a:b", "actor": "t", "zz": 1, "labels": {"t": ["x"]},
                    "memories": [{"id": "m", "kind": "observation", "summary": "S.", "evidence": "E."}]}),
        )
        .expect_err("an unknown argument is refused");
        assert_eq!(error.feedback[0]["code"], "UNKNOWN_ARGUMENT");
        assert_eq!(error.feedback[0]["field"], "zz");
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
