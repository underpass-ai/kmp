//! How a schema refusal is said. Kept apart from the check so the check reads
//! as the law and this reads as the voice.

use serde_json::Value;

/// The JSON type of a value, as JSON Schema names it.
pub(crate) fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::Null => "null",
    }
}

/// Whether a declared `type` holds this value. An integer is a number
/// written without a decimal point: `1.0` is refused where an integer is
/// declared, because the readers behind the schema take it as absent.
pub(crate) fn type_allows(declared: &str, value: &Value) -> bool {
    let kind = json_kind(value);
    declared == kind || (declared == "number" && kind == "integer")
}

pub(crate) fn a_kind(kind: &str) -> &'static str {
    match kind {
        "string" => "a string",
        "array" => "an array",
        "object" => "an object",
        "boolean" => "a JSON boolean",
        "integer" => "an integer",
        "number" => "a number",
        "null" => "null",
        _ => "a value of the declared type",
    }
}

/// `a string: "x"`, `a number: 1`, `null`… with long values cut short.
pub(crate) fn received(value: &Value) -> String {
    let text = shortened(&value.to_string());
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => format!("a boolean: {text}"),
        Value::Number(_) => format!("a number: {text}"),
        Value::String(_) => format!("a string: {text}"),
        Value::Array(_) => format!("an array: {text}"),
        Value::Object(_) => format!("an object: {text}"),
    }
}

/// Allowed values, as an agent would type them.
pub(crate) fn listed(values: &[Value]) -> String {
    const SHOWN: usize = 16;
    let mut words = values
        .iter()
        .take(SHOWN)
        .map(|value| match value {
            Value::String(text) => format!("`{text}`"),
            other => format!("`{other}`"),
        })
        .collect::<Vec<_>>();
    if values.len() > SHOWN {
        words.push(format!("and {} more", values.len() - SHOWN));
    }
    words.join(", ")
}

/// "when `continuation` is given, " for a condition that names arguments.
pub(crate) fn condition_wording(condition: &Value, holds: bool) -> Option<String> {
    if let Some(names) = required_names(condition)
        && condition.get("properties").is_none()
    {
        let verb = if holds { "is given" } else { "is not given" };
        return Some(format!("When {names} {verb}, "));
    }
    let inner = condition.get("not")?;
    let names = required_names(inner)?;
    if inner.get("properties").is_some() {
        return None;
    }
    let verb = if holds { "is not given" } else { "is given" };
    Some(format!("When {names} {verb}, "))
}

/// What a `not` forbids, named by the arguments that are present.
pub(crate) fn not_wording(forbidden: &Value, value: &Value, path: &str) -> String {
    let mut named = Vec::new();
    collect_present(forbidden, value, &mut named);
    named.dedup();
    if named.is_empty() {
        return format!("`{path}` combines arguments that cannot be given together.");
    }
    let names = named
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("`{path}` cannot take {names} here.")
}

fn collect_present(forbidden: &Value, value: &Value, named: &mut Vec<String>) {
    for name in forbidden
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if value.get(name).is_some() {
            named.push(name.to_string());
        }
    }
    for branch in forbidden
        .get("anyOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect_present(branch, value, named);
    }
}

fn required_names(condition: &Value) -> Option<String> {
    let names = condition
        .get("required")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>();
    (!names.is_empty()).then(|| names.join(" and "))
}

fn shortened(text: &str) -> String {
    const LIMIT: usize = 80;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let cut = text.chars().take(LIMIT).collect::<String>();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn integers_and_numbers() {
        assert!(type_allows("integer", &json!(3)));
        assert!(!type_allows("integer", &json!(3.0)));
        assert!(type_allows("number", &json!(3)));
        assert!(!type_allows("string", &json!(3)));
    }

    #[test]
    fn conditions_name_their_arguments() {
        assert_eq!(
            condition_wording(&json!({"required": ["continuation"]}), true).as_deref(),
            Some("When `continuation` is given, ")
        );
        assert_eq!(
            condition_wording(&json!({"not": {"required": ["actor"]}}), true).as_deref(),
            Some("When `actor` is not given, ")
        );
        assert_eq!(
            condition_wording(&json!({"properties": {"rel": {"enum": ["x"]}}}), true),
            None
        );
    }

    #[test]
    fn a_forbidden_combination_names_what_is_present() {
        let forbidden = json!({"anyOf": [{"required": ["labels"]}, {"required": ["rank"]}]});
        assert_eq!(
            not_wording(&forbidden, &json!({"labels": {}, "x": 1}), "t"),
            "`t` cannot take `labels` here."
        );
        assert!(received(&json!("x".repeat(200))).ends_with('…'));
        assert_eq!(listed(&[json!("a"), json!(1)]), "`a`, `1`");
    }
}
