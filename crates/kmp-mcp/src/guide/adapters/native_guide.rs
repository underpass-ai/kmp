use crate::guide::adapters::file_guide_asset_repository::FileGuideAssetRepository;
use crate::guide::adapters::guide_cli_parser::GuideCliParser;
use crate::guide::adapters::mcp_guide_memory_gateway::McpGuideMemoryGateway;
use crate::guide::application::dto::guide_sync_receipt_dto::GuideSyncReceiptDto;
use crate::guide::application::use_cases::sync_guide::SyncGuide;
use crate::guide::domain::guide_error::GuideError;

pub struct NativeGuide;

impl NativeGuide {
    pub async fn execute(arguments: &[&str]) -> Result<GuideSyncReceiptDto, GuideError> {
        let request = GuideCliParser::parse(arguments)?;
        SyncGuide::new(&FileGuideAssetRepository, &McpGuideMemoryGateway)
            .execute(&request)
            .await
    }
}
