//! One vocabulary for the semantic writer's schema, validation and repair hints.
pub(crate) const WRITER_MEMORY_KINDS: &[&str] = &[
    "turn",
    "observation",
    "decision",
    "feedback",
    "semantic_delta",
    "constraint",
    "preference",
    "derived_value",
    "error_path",
    "success_path",
];
