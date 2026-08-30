use serde_json::{Value, json};

use kmp_domain::{MemoryRelationQuality, MemoryRelationSpec, RelationSemanticClass};

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
