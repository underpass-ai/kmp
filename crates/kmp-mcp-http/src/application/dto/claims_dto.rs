use serde::Deserialize;

use super::string_or_list_dto::StringOrListDto;

#[derive(Debug, Deserialize)]
pub(crate) struct ClaimsDto {
    pub sub: String,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub scope: StringOrListDto,
    #[serde(default)]
    pub kmp_abouts: StringOrListDto,
    #[serde(default)]
    pub kmp_scope_ids: StringOrListDto,
    #[serde(default)]
    pub kmp_ref_prefixes: StringOrListDto,
}
