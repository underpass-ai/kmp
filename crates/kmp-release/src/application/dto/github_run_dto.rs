use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct GithubRunDto {
    #[serde(rename = "databaseId")]
    pub database_id: u64,
    #[serde(default, rename = "headSha")]
    pub head_sha: String,
}
