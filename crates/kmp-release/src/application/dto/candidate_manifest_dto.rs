use serde::{Deserialize, Serialize};

use crate::application::dto::candidate_asset_dto::CandidateAssetDto;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CandidateManifestDto {
    pub contract: String,
    pub version: String,
    pub input_sha256: String,
    pub source_sha: String,
    pub source_ref: String,
    pub run_id: String,
    pub assets: Vec<CandidateAssetDto>,
}
