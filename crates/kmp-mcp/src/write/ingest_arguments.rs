use serde_json::{Map, Value};

pub(super) fn required_string(value: Option<&Value>, key: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing required argument `{key}`"))
}
pub(super) fn required_array<'a>(
    value: Option<&'a Value>,
    key: &str,
) -> Result<&'a [Value], String> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing required array argument `{key}`"))?;
    if values.is_empty() {
        return Err(format!("required array argument `{key}` must not be empty"));
    }
    Ok(values)
}
pub(super) fn optional_array<'a>(
    value: Option<&'a Value>,
    key: &str,
) -> Result<&'a [Value], String> {
    match value {
        Some(value) => value
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| format!("argument `{key}` must be an array")),
        None => Ok(&[]),
    }
}
pub(super) fn required_object_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .as_object()
        .and_then(|object| object.get(key.rsplit('.').next().unwrap_or(key)))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing required argument `{key}`"))
}
pub(super) fn required_map_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a str, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing required argument `{path}`"))
}
