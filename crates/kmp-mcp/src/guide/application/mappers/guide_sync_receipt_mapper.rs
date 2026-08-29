use crate::guide::application::dto::guide_sync_receipt_dto::GuideSyncReceiptDto;
use crate::guide::domain::guide_sync_mode::GuideSyncMode;

pub struct GuideSyncReceiptMapper;

impl GuideSyncReceiptMapper {
    pub fn to_text(receipt: &GuideSyncReceiptDto) -> String {
        match receipt.mode() {
            GuideSyncMode::Apply => format!(
                "KMP guide: converged {} immutable guide memories into the selected store",
                receipt.request_count()
            ),
            GuideSyncMode::DryRun => format!(
                "KMP guide: would converge {} immutable guide memories into the selected store",
                receipt.request_count()
            ),
        }
    }
}
