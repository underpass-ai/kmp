//! A label as the writer emits it, and the two checks every label passes
//! before it becomes a coordinate: a key a filter can name, and a unique
//! membership under that key.

/// One label the writer emits as a coordinate: its key is the dimension
/// kind, its value the scope id, and `field` names the argument it came
/// from so a refusal points at what to change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WriterLabel {
    pub(super) key: String,
    pub(super) value: String,
    pub(super) field: String,
    pub(super) title: &'static str,
}

impl WriterLabel {
    pub(super) fn new(
        key: impl Into<String>,
        value: impl Into<String>,
        field: impl Into<String>,
        title: &'static str,
    ) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            field: field.into(),
            title,
        }
    }
}

/// Duplicate membership is an error; the same value under another key is
/// a separate dimension and is valid.
pub(super) fn validate_distinct_labels(labels: &[WriterLabel]) -> Result<(), String> {
    let mut seen = std::collections::BTreeMap::new();
    for label in labels {
        if let Some(first) = seen.insert((&label.key, &label.value), &label.field) {
            return Err(format!(
                "{first} and {} repeat `{}={}`",
                label.field, label.key, label.value
            ));
        }
    }
    Ok(())
}

/// All label inputs use arrays, including single values. This keeps the
/// public schema uniform and allows multiple memberships under every key.
pub(super) fn label_values(value: &serde_json::Value, field: &str) -> Result<Vec<String>, String> {
    let values = value
        .as_array()
        .filter(|values| !values.is_empty())
        .ok_or_else(|| format!("`{field}` must be a non-empty array of strings"))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let value = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("`{field}[{index}]` must be a non-empty string"))?;
        if value.chars().any(char::is_control) {
            return Err(format!(
                "`{field}[{index}]` cannot contain control characters"
            ));
        }
        if !seen.insert(value) {
            return Err(format!("`{field}[{index}]` repeats `{value}`"));
        }
        result.push(value.to_string());
    }
    Ok(result)
}

/// A label key is an identifier a filter can name: lowercase letters,
/// digits, `_`, `.` and `-`, starting with a letter, at most 64 characters.
pub(super) fn validate_label_key(key: &str) -> Result<(), String> {
    validate_label_key_at("labels", key)
}

/// The same check for a key given under another argument, so the refusal
/// names the field it came from.
pub(super) fn validate_label_key_at(field: &str, key: &str) -> Result<(), String> {
    let mut chars = key.chars();
    let first_is_letter = chars.next().is_some_and(|first| first.is_ascii_lowercase());
    let rest_is_plain = key
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '.' | '-'));
    if key.is_empty() || key.len() > 64 || !first_is_letter || !rest_is_plain {
        return Err(format!(
            "`{field}.{key}` is not a label key: use lowercase letters, digits, `_`, `.` or `-`, starting with a letter, at most 64 characters"
        ));
    }
    Ok(())
}
