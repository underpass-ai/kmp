use serde::Serialize;

/// Stable output projection for one black-box engine proof.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LifecycleEngineDto {
    pub consumer: String,
    pub executable: String,
    pub version: String,
    pub tool_count: usize,
}
