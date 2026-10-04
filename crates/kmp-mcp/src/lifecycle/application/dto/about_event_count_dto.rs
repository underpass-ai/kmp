use serde::Serialize;

/// One about in a store and its event count, as `memories --json` prints it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AboutEventCountDto {
    pub about: String,
    pub events: u64,
}
