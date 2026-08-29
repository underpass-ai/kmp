use serde::Deserialize;

use crate::application::dto::guide_source_entry_dto::GuideSourceEntryDto;
use crate::application::dto::guide_source_relation_dto::GuideSourceRelationDto;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideSourceAboutDto {
    pub about: String,
    pub audience: String,
    pub entries: Vec<GuideSourceEntryDto>,
    pub relations: Vec<GuideSourceRelationDto>,
}
