use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSegmentKind {
    SourceCode,
    Math,
    Url,
    Text,
}

impl EvidenceSegmentKind {
    pub fn precedence(self) -> u8 {
        match self {
            Self::SourceCode => 0,
            Self::Math => 1,
            Self::Url => 2,
            Self::Text => 3,
        }
    }

    pub fn is_interpretable_text(self) -> bool {
        self == Self::Text
    }

    pub fn is_protected(self) -> bool {
        !self.is_interpretable_text()
    }
}
