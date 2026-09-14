use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewRevision {
    pub about: String,
    pub view: String,
    pub revision: u64,
}
