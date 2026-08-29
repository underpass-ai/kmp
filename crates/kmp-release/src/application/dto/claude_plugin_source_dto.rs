use serde::Deserialize;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct ClaudePluginSourceDto {
    pub source: String,
    pub url: String,
    pub path: String,
    #[serde(rename = "ref")]
    pub reference: String,
}
