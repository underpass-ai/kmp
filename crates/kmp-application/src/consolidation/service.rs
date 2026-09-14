use super::{CaptureConsolidationSources, ReadConsolidationView, WriteConsolidationView};
use kmp_domain::{
    PortError,
    consolidation::{
        ConsolidatedView, ConsolidationRead, ConsolidationSource, ConsolidationStore,
        ConsolidationWrite,
    },
};
use std::sync::Arc;

/// Thin composition facade for independently owned capture, write and read use cases.
pub struct ConsolidationApplicationService {
    capture: CaptureConsolidationSources,
    write: WriteConsolidationView,
    read: ReadConsolidationView,
}
impl ConsolidationApplicationService {
    pub fn new(store: Arc<dyn ConsolidationStore>) -> Self {
        Self {
            capture: CaptureConsolidationSources::new(store.clone()),
            write: WriteConsolidationView::new(store.clone()),
            read: ReadConsolidationView::new(store),
        }
    }
    pub async fn sources(
        &self,
        about: String,
        refs: Vec<String>,
    ) -> Result<Vec<ConsolidationSource>, PortError> {
        self.capture.execute(about, refs).await
    }
    pub async fn write(&self, command: ConsolidationWrite) -> Result<ConsolidatedView, PortError> {
        self.write.execute(command).await
    }
    pub async fn read(
        &self,
        about: String,
        view: String,
        revision: Option<u64>,
    ) -> Result<ConsolidationRead, PortError> {
        self.read.execute(about, view, revision).await
    }
}
