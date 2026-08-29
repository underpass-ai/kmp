use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CandidateAssetDto {
    pub name: String,
    pub sha256: String,
    pub size: u64,
}
