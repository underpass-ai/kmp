use serde::Deserialize;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideSourceRelationDto {
    pub from: String,
    pub to: String,
    pub rel: String,
    pub class: String,
    pub why: String,
    pub evidence: String,
}
