use serde::{Deserialize, Serialize};

use super::evidence_fragment::EvidenceFragment;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceInterpretationInput {
    pub fragments: Vec<EvidenceFragment>,
}

impl EvidenceInterpretationInput {
    pub fn new(fragments: Vec<EvidenceFragment>) -> Self {
        Self { fragments }
    }
}
