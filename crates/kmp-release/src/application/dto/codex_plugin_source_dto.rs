use serde::Deserialize;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct CodexPluginSourceDto {
    pub source: String,
    pub path: String,
}
