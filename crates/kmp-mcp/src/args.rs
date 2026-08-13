use serde_json::Value;

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
