use serde::Deserialize;

use crate::application::dto::codex_plugin_source_dto::CodexPluginSourceDto;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct CodexMarketplacePluginDto {
    pub name: String,
    pub source: CodexPluginSourceDto,
}
