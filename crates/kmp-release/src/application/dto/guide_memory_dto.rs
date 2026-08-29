use serde::Deserialize;

use crate::application::dto::guide_entry_dto::GuideEntryDto;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideMemoryDto {
    pub entries: Vec<GuideEntryDto>,
}
