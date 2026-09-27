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

/// Said when a record carries no membership at all. The example is a shape,
/// not a default: no membership is ever invented for a record.
pub(super) const LABELS_REQUIRED: &str = "labels must declare at least one key/value \
     membership for temporal navigation. Add `labels` at the top level for every record or \
     inside this record, for example \"labels\": {\"topic\": [\"write-path\"]}; reuse the \
     about's own keys (kmp_wake lists them). No membership is invented.";

/// The labels a write emits, in the order the ingest has always carried
/// them: the well-known task, process and episode scopes first, then the
/// caller's own `labels` by key. Packet records can supply only labels;
/// the single-current form provides the well-known process/task/episode
/// memberships through scope. Neither path invents a label.
pub(super) fn writer_labels(
    process: Option<&str>,
    task: Option<&str>,
    episode: Option<&str>,
    labels: Option<&serde_json::Value>,
) -> Result<Vec<WriterLabel>, String> {
    let mut emitted = Vec::new();
    if let Some(task) = task {
        emitted.push(WriterLabel::new(
            "task",
            task,
            "scope.task",
            "Kernel write task",
        ));
    }
    if let Some(process) = process {
        emitted.push(WriterLabel::new(
            "agentic_process",
            process,
            "scope.process",
            "Kernel write process",
        ));
    }
    if let Some(episode) = episode {
        emitted.push(WriterLabel::new(
            "agentic_episode",
            episode,
            "scope.episode",
            "Kernel write episode",
        ));
    }
    if let Some(labels) = labels {
        let object = labels.as_object().ok_or_else(|| {
            "`labels` must map each key to a non-empty array of strings".to_string()
        })?;
        let mut own = object.iter().collect::<Vec<_>>();
        own.sort_by(|left, right| left.0.cmp(right.0));
        for (key, value) in own {
            validate_label_key(key)?;
            let field = format!("labels.{key}");
            for value in label_values(value, &field)? {
                emitted.push(WriterLabel::new(key, value, &field, "Kernel write label"));
            }
        }
    }
    validate_distinct_labels(&emitted)?;
    if emitted.is_empty() {
        return Err(LABELS_REQUIRED.to_string());
    }
    Ok(emitted)
}
