use crate::guide::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use crate::guide::domain::guide_error::GuideError;
use crate::guide::domain::guide_plugin_root::GuidePluginRoot;

pub trait GuideAssetRepository {
    fn load(&self, root: &GuidePluginRoot) -> Result<Vec<GuideRequestDocumentDto>, GuideError>;
}
