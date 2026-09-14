use kmp_domain::{
    PortError,
    consolidation::{ConsolidatedView, ConsolidationStore, ConsolidationWrite},
};
use std::sync::Arc;

pub struct WriteConsolidationView {
    store: Arc<dyn ConsolidationStore>,
}
impl WriteConsolidationView {
    pub fn new(store: Arc<dyn ConsolidationStore>) -> Self {
        Self { store }
    }
    pub async fn execute(
        &self,
        command: ConsolidationWrite,
    ) -> Result<ConsolidatedView, PortError> {
        self.store.write_consolidation(command).await
    }
}
