use serde::Deserialize;

use crate::application::dto::claude_plugin_source_dto::ClaudePluginSourceDto;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct ClaudeMarketplacePluginDto {
    pub name: String,
    pub description: String,
    pub source: ClaudePluginSourceDto,
}
