use crate::guide::domain::guide_sync_mode::GuideSyncMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideSyncReceiptDto {
    request_count: usize,
    mode: GuideSyncMode,
}

impl GuideSyncReceiptDto {
    pub fn new(request_count: usize, mode: GuideSyncMode) -> Self {
        Self {
            request_count,
            mode,
        }
    }

    pub fn request_count(&self) -> usize {
        self.request_count
    }

    pub fn mode(&self) -> GuideSyncMode {
        self.mode
    }
}
