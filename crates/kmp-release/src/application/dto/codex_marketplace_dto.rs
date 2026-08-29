use serde::Deserialize;

use crate::application::dto::codex_marketplace_plugin_dto::CodexMarketplacePluginDto;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct CodexMarketplaceDto {
    pub plugins: Vec<CodexMarketplacePluginDto>,
}
