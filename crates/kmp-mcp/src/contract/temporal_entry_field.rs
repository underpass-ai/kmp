//! Selectable entry fields; identity is always present for navigation.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TemporalEntryField {
    Ref,
    Kind,
    Text,
    Coordinates,
    Metadata,
}

impl TemporalEntryField {
    pub(crate) const ALL: [Self; 5] = [
        Self::Ref,
        Self::Kind,
        Self::Text,
        Self::Coordinates,
        Self::Metadata,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ref => "ref",
            Self::Kind => "kind",
            Self::Text => "text",
            Self::Coordinates => "coordinates",
            Self::Metadata => "metadata",
        }
    }

    pub(crate) fn is_identity(self) -> bool {
        matches!(self, Self::Ref | Self::Kind)
    }
}
