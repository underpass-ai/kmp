//! The writer relation vocabulary, as the surface advertises it.
//!
//! One concept, and the only place in this adapter that reads the domain. The
//! relation names and their prose are *projected* from
//! `KnownMemoryRelationType` rather than restated here, so the advertised
//! vocabulary cannot drift from the one the writer enforces. Adding a relation
//! type in the domain changes this description, and therefore the pinned
//! contract fixtures — that is the projection working, not a regression.

use serde_json::{Value, json};

use kmp_domain::KnownMemoryRelationType;

pub(crate) fn writer_relation_names() -> Vec<&'static str> {
    KnownMemoryRelationType::writer_relation_types()
        .iter()
        .map(|relation_type| relation_type.as_str())
        .collect()
}
/// The relation vocabulary, projected from the kernel's own writer spec so
/// this documentation can never drift from what the kernel validates. The
/// relation is where KMP carries the why; a model that only sees a bare enum
/// writes connected-but-unexplained memory, which is the failure mode the
/// spec exists to prevent.
pub(crate) fn relation_vocabulary_description(header: &str) -> String {
    let mut description = format!(
        "{header} The relation carries the explanation: non-structural classes require why, \
         evidence and confidence. Prefer rich types — anemic types are an honest fallback for \
         when no richer semantic dependency can be proven, never a default. Vocabulary \
         (quality; allowed classes; when to use):"
    );
    for spec in KnownMemoryRelationType::writer_relation_types()
        .iter()
        .filter_map(|relation_type| relation_type.writer_spec())
    {
        let classes = spec
            .allowed_classes()
            .iter()
            .map(|class| class.as_str())
            .collect::<Vec<_>>()
            .join("|");
        description.push_str(&format!(
            " {} ({}; {}; {}).",
            spec.relation_type().as_str(),
            spec.quality().as_str(),
            classes,
            spec.reason()
        ));
    }
    description
}
pub(crate) fn semantic_class_schema() -> Value {
    json!({
        "type": "string",
        "enum": ["structural", "causal", "motivational", "procedural", "evidential", "constraint"],
        "description": "What the link explains: structural = containment/membership, no proof \
                        required; causal = one memory triggered or produced another; \
                        motivational = one memory justifies or authorizes another; procedural = \
                        how something was executed, or plain succession; evidential = validates, \
                        proves, contradicts or verifies; constraint = limits or shapes another \
                        memory."
    })
}
