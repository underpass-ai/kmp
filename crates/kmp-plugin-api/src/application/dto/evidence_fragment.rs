use serde::{Deserialize, Serialize};

/// Evidence crossing the kernel/plugin boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceFragment {
    pub ref_id: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl EvidenceFragment {
    pub fn new(ref_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            ref_id: ref_id.into(),
            text: text.into(),
            source: None,
        }
    }
}
