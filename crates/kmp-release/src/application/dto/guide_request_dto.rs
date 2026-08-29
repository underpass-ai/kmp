use serde::Deserialize;

use crate::application::dto::guide_memory_dto::GuideMemoryDto;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideRequestDto {
    pub about: String,
    pub idempotency_key: String,
    pub memory: GuideMemoryDto,
}
