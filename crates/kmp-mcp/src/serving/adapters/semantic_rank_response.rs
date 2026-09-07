use serde::Deserialize;

/// Sidecar response carries only ranked identities and fingerprints.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SemanticRankResponse {
    pub model_revision: String,
    pub question_sha256: String,
    pub candidates: Vec<(String, String)>,
}
