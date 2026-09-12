use prost_types::Timestamp;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Reads typed JSON fields and reports their caller-visible paths.
pub(super) struct JsonFieldReader;

impl JsonFieldReader {
    pub(super) fn object<'a>(
        value: &'a Value,
        path: &str,
    ) -> Result<&'a Map<String, Value>, String> {
        value
            .as_object()
            .ok_or_else(|| format!("`{path}` must be a JSON object"))
    }

    pub(super) fn required_object_field<'a>(
        object: &'a Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<&'a Map<String, Value>, String> {
        object
            .get(key)
            .ok_or_else(|| format!("missing required object argument `{path}`"))
            .and_then(|value| {
                value
                    .as_object()
                    .ok_or_else(|| format!("argument `{path}` must be an object"))
            })
    }

    pub(super) fn optional_object_field<'a>(
        object: &'a Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<Option<&'a Map<String, Value>>, String> {
        object
            .get(key)
            .map(|value| {
                value
                    .as_object()
                    .ok_or_else(|| format!("argument `{path}` must be an object"))
            })
            .transpose()
    }

    pub(super) fn required_array_field<'a>(
        object: &'a Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<&'a [Value], String> {
        let values = object
            .get(key)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("missing required array argument `{path}`"))?;
        if values.is_empty() {
            return Err(format!(
                "required array argument `{path}` must not be empty"
            ));
        }
        Ok(values)
    }

    pub(super) fn optional_array_field<'a>(
        object: &'a Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<&'a [Value], String> {
        match object.get(key) {
            Some(value) => value
                .as_array()
                .map(Vec::as_slice)
                .ok_or_else(|| format!("argument `{path}` must be an array")),
            None => Ok(&[]),
        }
    }

    pub(super) fn required_string_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<String, String> {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .ok_or_else(|| format!("missing required argument `{path}`"))
    }

    pub(super) fn optional_string_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<Option<String>, String> {
        object
            .get(key)
            .map(|value| {
                value
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .ok_or_else(|| format!("argument `{path}` must be a non-empty string"))
            })
            .transpose()
    }

    pub(super) fn optional_string_array_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<Vec<String>, String> {
        let Some(value) = object.get(key) else {
            return Ok(Vec::new());
        };
        let values = value
            .as_array()
            .ok_or_else(|| format!("argument `{path}` must be an array"))?;
        values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .ok_or_else(|| format!("argument `{path}[{index}]` must be a non-empty string"))
            })
            .collect()
    }

    pub(super) fn optional_metadata_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<HashMap<String, String>, String> {
        let Some(metadata) = Self::optional_object_field(object, key, path)? else {
            return Ok(HashMap::new());
        };
        metadata
            .iter()
            .map(|(key, value)| {
                let value = value
                    .as_str()
                    .ok_or_else(|| format!("argument `{path}.{key}` must be a string"))?;
                Ok((key.clone(), value.to_string()))
            })
            .collect()
    }

    pub(super) fn required_timestamp_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<Timestamp, String> {
        let value = Self::required_string_field(object, key, path)?;
        Self::parse_timestamp(&value, path)
    }

    pub(super) fn optional_timestamp_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<Option<Timestamp>, String> {
        Self::optional_string_field(object, key, path)?
            .map(|value| Self::parse_timestamp(&value, path))
            .transpose()
    }

    fn parse_timestamp(value: &str, path: &str) -> Result<Timestamp, String> {
        value
            .parse::<Timestamp>()
            .map_err(|error| format!("argument `{path}` must be an RFC3339 timestamp: {error}"))
    }

    pub(super) fn optional_bool_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<Option<bool>, String> {
        object
            .get(key)
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| format!("argument `{path}` must be a boolean"))
            })
            .transpose()
    }

    pub(super) fn optional_u32_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<Option<u32>, String> {
        let Some(value) = object.get(key) else {
            return Ok(None);
        };
        let value = value
            .as_u64()
            .ok_or_else(|| format!("argument `{path}` must be an integer"))?;
        u32::try_from(value)
            .map(Some)
            .map_err(|_| format!("argument `{path}` must fit in uint32"))
    }

    pub(super) fn optional_u64_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<Option<u64>, String> {
        object
            .get(key)
            .map(|value| {
                value
                    .as_u64()
                    .ok_or_else(|| format!("argument `{path}` must be a non-negative integer"))
            })
            .transpose()
    }

    pub(super) fn optional_positive_u32_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<Option<u32>, String> {
        let value = Self::optional_u32_field(object, key, path)?;
        if value == Some(0) {
            return Err(format!("argument `{path}` must be greater than zero"));
        }
        Ok(value)
    }
}
