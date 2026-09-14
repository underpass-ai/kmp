use kmp_domain::{
    PortError,
    consolidation::{ConsolidationSource, ConsolidationStore},
};
use std::sync::Arc;

pub struct CaptureConsolidationSources {
    store: Arc<dyn ConsolidationStore>,
}
impl CaptureConsolidationSources {
    pub fn new(store: Arc<dyn ConsolidationStore>) -> Self {
        Self { store }
    }
    pub async fn execute(
        &self,
        about: String,
        refs: Vec<String>,
    ) -> Result<Vec<ConsolidationSource>, PortError> {
        self.store.consolidation_sources(about, refs).await
    }
}
