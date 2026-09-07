use serde::Serialize;

/// Stored text offered to an optional encoder after scope/time admission.
/// This is an adapter boundary value, never a generated memory or answer.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticSource {
    pub entry_ref: String,
    pub text: String,
    pub text_sha256: String,
}
