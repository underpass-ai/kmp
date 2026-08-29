use std::path::Path;

use serde_json::Value;

use crate::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use crate::application::dto::guide_tool_call_dto::GuideToolCallDto;
use crate::application::dto::guide_tool_dto::GuideToolDto;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;

pub trait GuideEngine {
    fn version(&self) -> Result<ReleaseVersion, ReleaseError>;
    fn live_tools(&self, data_dir: &Path) -> Result<Vec<GuideToolDto>, ReleaseError>;
    fn ingest(
        &self,
        requests: &[GuideRequestDocumentDto],
        data_dir: Option<&Path>,
    ) -> Result<(), ReleaseError>;
    fn export(&self, data_dir: &Path, destination: &Path) -> Result<(), ReleaseError>;
    fn import(&self, data_dir: &Path, bundle: &Path) -> Result<(), ReleaseError>;
    fn call_tools(
        &self,
        data_dir: &Path,
        calls: &[GuideToolCallDto],
    ) -> Result<Vec<Value>, ReleaseError>;
}
