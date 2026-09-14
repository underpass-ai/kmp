use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceRequest {
    pub about: String,
    pub refs: Vec<String>,
}
