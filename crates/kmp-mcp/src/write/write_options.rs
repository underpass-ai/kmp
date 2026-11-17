//! The two switches every write carries in `options`: whether it commits,
//! and how strictly it is judged.
//!
//! They are read by type and never coerced. `dry_run: "true"` used to fall
//! through `as_bool` to the default, so a caller who asked for a preview got
//! a commit (QA, 13 nov 2026). A value of the wrong type is refused at its
//! own field, before anything is planned, so it cannot reach storage.

use serde_json::{Map, Value};

use super::json_value_type::JsonValueType;
use super::validation_error::WriteValidationError;

/// The write switches, as the caller typed them or as their defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WriteOptions {
    /// Validate and report without committing. Defaults to false: a tool
    /// called write commits, and the preview is opted into.
    pub(crate) dry_run: bool,
    /// Defaults to true.
    pub(crate) strict: bool,
}

impl WriteOptions {
    /// Reads `options` from the tool arguments. Absent switches take their
    /// defaults; a present one must be a JSON boolean, and `options` itself,
    /// when present, a JSON object.
    pub(crate) fn from_arguments(
        arguments: &Map<String, Value>,
    ) -> Result<Self, WriteValidationError> {
        let Some(options) = arguments.get("options") else {
            return Ok(Self::default());
        };
        let options = options.as_object().ok_or_else(|| {
            WriteValidationError::wrong_type("options", JsonValueType::Object, options)
        })?;
        Ok(Self {
            dry_run: switch(options, "dry_run")?.unwrap_or(false),
            strict: switch(options, "strict")?.unwrap_or(true),
        })
    }
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            strict: true,
        }
    }
}

fn switch(options: &Map<String, Value>, key: &str) -> Result<Option<bool>, WriteValidationError> {
    options
        .get(key)
        .map(|value| {
            value.as_bool().ok_or_else(|| {
                WriteValidationError::wrong_type(
                    &format!("options.{key}"),
                    JsonValueType::Boolean,
                    value,
                )
            })
        })
        .transpose()
}
