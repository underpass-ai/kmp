use serde::Deserialize;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideSourceEntryDto {
    pub id: String,
    pub kind: String,
    pub depth: String,
    pub text: String,
    pub evidence: String,
}
