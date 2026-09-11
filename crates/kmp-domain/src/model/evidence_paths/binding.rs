/// A task obligation at a path position. Membership never creates identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidencePathBinding {
    Label { at: u32, name: String, key: String },
    Reference { at: u32, name: String },
}

impl EvidencePathBinding {
    pub fn at(&self) -> u32 {
        match self {
            Self::Label { at, .. } | Self::Reference { at, .. } => *at,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Label { name, .. } | Self::Reference { name, .. } => name,
        }
    }
    pub(super) fn label_key(&self) -> Option<&str> {
        match self {
            Self::Label { key, .. } => Some(key),
            Self::Reference { .. } => None,
        }
    }
}
