use super::json_fields::JsonFieldReader;
use kmp_proto::v1beta1::{MemoryConfidence, MemorySemanticClass, MemorySourceKind};
use serde_json::{Map, Value};

/// Maps the existing memory enums wire values.
pub(super) struct MemoryEnumMapper;

impl MemoryEnumMapper {
    pub(super) fn semantic_class_from_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<i32, String> {
        Ok(
            match JsonFieldReader::required_string_field(object, key, path)?.as_str() {
                "structural" => MemorySemanticClass::Structural as i32,
                "causal" => MemorySemanticClass::Causal as i32,
                "motivational" => MemorySemanticClass::Motivational as i32,
                "procedural" => MemorySemanticClass::Procedural as i32,
                "evidential" => MemorySemanticClass::Evidential as i32,
                "constraint" => MemorySemanticClass::Constraint as i32,
                other => return Err(format!("invalid memory relation class `{other}`")),
            },
        )
    }

    pub(super) fn confidence_from_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<i32, String> {
        Ok(
            match JsonFieldReader::optional_string_field(object, key, path)?.as_deref() {
                None => MemoryConfidence::Unspecified as i32,
                Some("high") => MemoryConfidence::High as i32,
                Some("medium") => MemoryConfidence::Medium as i32,
                Some("low") => MemoryConfidence::Low as i32,
                Some("unknown") => MemoryConfidence::Unknown as i32,
                Some(other) => return Err(format!("invalid memory relation confidence `{other}`")),
            },
        )
    }

    pub(super) fn source_kind_from_field(
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Result<i32, String> {
        Ok(
            match JsonFieldReader::required_string_field(object, key, path)?.as_str() {
                "human" => MemorySourceKind::Human as i32,
                "agent" => MemorySourceKind::Agent as i32,
                "projection" => MemorySourceKind::Projection as i32,
                "derived" => MemorySourceKind::Derived as i32,
                other => return Err(format!("invalid memory provenance source_kind `{other}`")),
            },
        )
    }
}
