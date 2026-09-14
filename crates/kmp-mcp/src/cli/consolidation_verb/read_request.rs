use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadRequest {
    pub about: String,
    pub view: String,
    pub revision: Option<u64>,
}
