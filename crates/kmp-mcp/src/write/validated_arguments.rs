//! Field-aware versions of the shared JSON readers for semantic compilation.

use serde_json::{Map, Value};

use super::arguments;
use super::validation_error::WriteValidationError;

pub(super) use arguments::{
    optional_array, optional_map_string, optional_string, reject_duplicate_ref,
    validate_confidence, validate_intent, validate_node_kind, validate_semantic_class,
};

pub(crate) fn required_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<String, WriteValidationError> {
    arguments::required_string(object, key).map_err(|error| {
        WriteValidationError::new(error)
            .at(key)
            .code("REQUIRED_FIELD")
    })
}

pub(super) fn required_object<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Map<String, Value>, WriteValidationError> {
    arguments::required_object(object, key).map_err(|error| {
        WriteValidationError::new(error)
            .at(key)
            .code("REQUIRED_FIELD")
    })
}

pub(crate) fn required_map_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a str, WriteValidationError> {
    arguments::required_map_string(object, key, path).map_err(|error| {
        WriteValidationError::new(error)
            .at(path)
            .code("REQUIRED_FIELD")
    })
}

pub(super) fn required_relation_string<'a>(
    relation: &'a Map<String, Value>,
    key: &str,
    semantic_class: &str,
    index: usize,
) -> Result<&'a str, WriteValidationError> {
    arguments::required_relation_string(relation, key, semantic_class, index).map_err(|error| {
        WriteValidationError::new(error)
            .at(format!("connect_to[{index}].{key}"))
            .code("RELATION_PROOF_REQUIRED")
    })
}
