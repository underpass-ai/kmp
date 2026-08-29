use crate::guide::application::dto::guide_sync_receipt_dto::GuideSyncReceiptDto;
use crate::guide::application::dto::guide_sync_request_dto::GuideSyncRequestDto;
use crate::guide::domain::guide_error::GuideError;
use crate::guide::ports::guide_asset_repository::GuideAssetRepository;
use crate::guide::ports::guide_memory_gateway::GuideMemoryGateway;

pub struct SyncGuide<'a, R, G> {
    assets: &'a R,
    memory: &'a G,
}

impl<'a, R, G> SyncGuide<'a, R, G>
where
    R: GuideAssetRepository,
    G: GuideMemoryGateway,
{
    pub fn new(assets: &'a R, memory: &'a G) -> Self {
        Self { assets, memory }
    }

    pub async fn execute(
        &self,
        request: &GuideSyncRequestDto,
    ) -> Result<GuideSyncReceiptDto, GuideError> {
        let requests = self.assets.load(request.plugin_root())?;
        if request.mode().should_apply() {
            self.memory.converge(&requests).await?;
        }
        Ok(GuideSyncReceiptDto::new(requests.len(), request.mode()))
    }
}
