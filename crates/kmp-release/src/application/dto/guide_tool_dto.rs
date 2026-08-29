use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct GuideToolDto {
    pub name: String,
    pub description: String,
}
