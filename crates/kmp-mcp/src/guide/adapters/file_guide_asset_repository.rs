use std::collections::BTreeSet;

use serde_json::Value;

use crate::guide::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use crate::guide::domain::guide_error::GuideError;
use crate::guide::domain::guide_plugin_root::GuidePluginRoot;
use crate::guide::domain::shipped_guide_abouts::ShippedGuideAbouts;
use crate::guide::ports::guide_asset_repository::GuideAssetRepository;

pub struct FileGuideAssetRepository;

impl GuideAssetRepository for FileGuideAssetRepository {
    fn load(&self, root: &GuidePluginRoot) -> Result<Vec<GuideRequestDocumentDto>, GuideError> {
        let requests_path = root.requests_path();
        let text = std::fs::read_to_string(&requests_path).map_err(|error| {
            GuideError::invalid(format!(
                "could not read guide requests `{}`: {error}",
                requests_path.display()
            ))
        })?;
        let bodies: Vec<Value> = serde_json::from_str(&text).map_err(|error| {
            GuideError::invalid(format!(
                "guide requests `{}` are invalid: {error}",
                requests_path.display()
            ))
        })?;
        let requests = bodies
            .into_iter()
            .map(GuideRequestDocumentDto::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let abouts = requests
            .iter()
            .map(GuideRequestDocumentDto::about)
            .collect::<BTreeSet<_>>();
        if requests.len() != 2 || abouts != BTreeSet::from(ShippedGuideAbouts::all()) {
            return Err(GuideError::invalid(
                "guide requests must contain exactly guide:kmp and guide:kmp-agent",
            ));
        }

        let bundle_path = root.bundle_path();
        let bundle = std::fs::read_to_string(&bundle_path).map_err(|error| {
            GuideError::invalid(format!(
                "could not read guide bundle `{}`: {error}",
                bundle_path.display()
            ))
        })?;
        let header = kmp_embedded::verify_bundle(&bundle)
            .map_err(|error| GuideError::invalid(format!("guide bundle is invalid: {error}")))?;
        if header.event_count != 2 || header.abouts != ShippedGuideAbouts::owned() {
            return Err(GuideError::invalid(
                "guide bundle must contain exactly the two guide memories",
            ));
        }
        Ok(requests)
    }
}
