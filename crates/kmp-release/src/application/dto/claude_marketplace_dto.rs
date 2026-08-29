use serde::Deserialize;

use crate::application::dto::claude_marketplace_plugin_dto::ClaudeMarketplacePluginDto;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct ClaudeMarketplaceDto {
    pub plugins: Vec<ClaudeMarketplacePluginDto>,
}
