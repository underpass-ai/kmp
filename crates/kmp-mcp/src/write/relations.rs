use serde_json::{Map, Value, json};

use kmp_domain::{MemoryRelationQuality, MemoryRelationSpec, RelationSemanticClass};

use super::json_value_type::JsonValueType;
use super::relation_quality::{class_names, relation_spec};
use super::validated_arguments::required_map_string;
use super::validation_error::WriteValidationError;

/// Complete only a choice already fixed by the domain. An explicit value is
/// never overwritten, and an ambiguous relation still needs the writer's choice.
pub(super) fn resolve_class<'a>(
    link: &'a Map<String, Value>,
    rel: &str,
    strict: bool,
) -> Result<&'a str, WriteValidationError> {
    if let Some(value) = link.get("class") {
        if !value.is_string() {
            return Err(WriteValidationError::wrong_type(
                "class",
                JsonValueType::String,
                value,
            ));
        }
        return required_map_string(link, "class", "class");
    }
    let spec = relation_spec(rel, strict).map_err(|error| {
        WriteValidationError::new(error)
            .at("rel")
            .code("INVALID_RELATION")
    })?;
    if let [class] = spec.classes {
        return Ok(class.as_str());
    }
    let choices = class_names(spec.classes);
    Err(WriteValidationError::new(format!(
        "relation `{rel}` needs an explicit class; choose from {} using the source evidence",
        choices.join(", ")
    ))
    .at("class")
    .code("RELATION_CLASS_REQUIRED")
    .allowed_values(choices))
}

pub(super) const NON_STRUCTURAL_RELATION_CLASSES: &[RelationSemanticClass] = &[
    RelationSemanticClass::Causal,
    RelationSemanticClass::Motivational,
    RelationSemanticClass::Procedural,
    RelationSemanticClass::Evidential,
    RelationSemanticClass::Constraint,
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedRelationSpec {
    pub(crate) quality: MemoryRelationQuality,
    pub(crate) classes: &'static [RelationSemanticClass],
    pub(crate) reason: &'static str,
}

impl From<MemoryRelationSpec> for ResolvedRelationSpec {
    fn from(value: MemoryRelationSpec) -> Self {
        Self {
            quality: value.quality(),
            classes: value.allowed_classes(),
            reason: value.reason(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn relation(
    from: &str,
    to: &str,
    rel: &str,
    semantic_class: &str,
    confidence: &str,
    why: &str,
    evidence: &str,
    sequence: u32,
) -> Value {
    // A structural link carries no rationale, and that is the writer's own
    // rule: `why` and `evidence` are required for every other class. The
    // canonical ingest mapper reads a *present but empty* string as a
    // malformed argument rather than an absent one, so emitting `""` here
    // rejected exactly the writes the rule allows. Omit the keys instead.
    let mut relation = json!({
        "from": from,
        "to": to,
        "rel": rel,
        "class": semantic_class,
        "confidence": confidence,
        "sequence": sequence
    });
    let fields = relation
        .as_object_mut()
        .expect("relation literal is a JSON object");
    if !why.trim().is_empty() {
        fields.insert("why".to_string(), json!(why));
    }
    if !evidence.trim().is_empty() {
        fields.insert("evidence".to_string(), json!(evidence));
    }
    relation
}
