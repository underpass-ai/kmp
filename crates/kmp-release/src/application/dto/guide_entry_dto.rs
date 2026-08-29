use serde::Deserialize;

use crate::application::dto::guide_entry_metadata_dto::GuideEntryMetadataDto;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideEntryDto {
    pub metadata: GuideEntryMetadataDto,
}
