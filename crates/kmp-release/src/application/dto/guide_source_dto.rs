use serde::Deserialize;

use crate::application::dto::guide_source_about_dto::GuideSourceAboutDto;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideSourceDto {
    pub schema_version: u32,
    pub guide_version: String,
    pub observed_at: String,
    pub abouts: Vec<GuideSourceAboutDto>,
}
