use super::{ContextGroup, compose};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// File/host boundary for the same pure composer used by embedded consumers.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionRequest {
    pub groups: Vec<ContextGroup>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

impl ProjectionRequest {
    pub fn project(&self) -> Result<Value, String> {
        compose(&self.groups, self.max_bytes.unwrap_or(usize::MAX))
    }
}
