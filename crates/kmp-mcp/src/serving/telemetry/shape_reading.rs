//! Shared JSON walking for the telemetry shapes: counts and numbers at a
//! path, never the text at it.

use serde_json::Value;

pub(super) fn array_len_at(root: Option<&Value>, path: &[&str]) -> usize {
    value_at(root, path)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

pub(super) fn number_at(root: Option<&Value>, path: &[&str]) -> u64 {
    value_at(root, path)
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

pub(super) fn value_at<'a>(root: Option<&'a Value>, path: &[&str]) -> Option<&'a Value> {
    let mut current = root?;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

pub(super) fn first_non_zero(values: &[usize]) -> usize {
    values.iter().copied().find(|value| *value > 0).unwrap_or(0)
}
