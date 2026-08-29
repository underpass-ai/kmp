use crate::application::dto::github_run_dto::GithubRunDto;
use crate::domain::release_error::ReleaseError;

pub struct GithubRunMapper;

impl GithubRunMapper {
    pub fn map_many(json: &str) -> Result<Vec<GithubRunDto>, ReleaseError> {
        serde_json::from_str(json).map_err(|error| {
            ReleaseError::invalid(format!("GitHub returned invalid workflow JSON: {error}"))
        })
    }
}
