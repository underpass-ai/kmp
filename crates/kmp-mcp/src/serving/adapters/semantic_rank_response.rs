use serde::Deserialize;

/// Sidecar response carries only ranked identities and fingerprints.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SemanticRankResponse {
    pub model_revision: String,
    pub question_sha256: String,
    pub candidates: Vec<(String, String)>,
    /// Present only when the sidecar supplies independent dense/BM25 ranks.
    /// An absent channel preserves the original single-ranking protocol.
    #[serde(default)]
    pub lexical_candidates: Option<Vec<(String, String)>>,
}
