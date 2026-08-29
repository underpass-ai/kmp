use serde::Deserialize;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideEntryMetadataDto {
    pub guide_version: String,
}
