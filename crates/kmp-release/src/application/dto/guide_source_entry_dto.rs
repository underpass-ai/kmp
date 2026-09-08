use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct GuideSourceEntryDto {
    pub id: String,
    pub kind: String,
    pub depth: String,
    #[serde(default)]
    pub text: String,
    pub text_file: Option<PathBuf>,
    pub evidence: String,
}
