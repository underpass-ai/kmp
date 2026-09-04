//! A label as the writer emits it, and the two checks every label passes
//! before it becomes a coordinate: a key a filter can name, and a value
//! not already used under another key in the same write.

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

/// Within an about a scope id names one label and keeps the kind of its
/// first use, which the ingest enforces. The writer refuses the reuse before
/// it reaches the kernel, naming both arguments.
pub(super) fn validate_distinct_label_values(labels: &[WriterLabel]) -> Result<(), String> {
    for (index, left) in labels.iter().enumerate() {
        for right in &labels[index + 1..] {
            if left.value == right.value {
                return Err(format!(
                    "{} and {} reuse `{}`; within an about a scope id names one label and keeps the kind of its first use, so one id cannot be two kinds",
                    left.field, right.field, left.value
                ));
            }
        }
    }
    Ok(())
}

/// A label key is an identifier a filter can name: lowercase letters,
/// digits, `_`, `.` and `-`, starting with a letter, at most 64 characters.
pub(super) fn validate_label_key(key: &str) -> Result<(), String> {
    let mut chars = key.chars();
    let first_is_letter = chars.next().is_some_and(|first| first.is_ascii_lowercase());
    let rest_is_plain = key
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '.' | '-'));
    if key.is_empty() || key.len() > 64 || !first_is_letter || !rest_is_plain {
        return Err(format!(
            "`labels.{key}` is not a label key: use lowercase letters, digits, `_`, `.` or `-`, starting with a letter, at most 64 characters"
        ));
    }
    Ok(())
}
