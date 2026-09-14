use kmp_domain::{
    PortError,
    consolidation::{ConsolidationRead, ConsolidationStore},
};
use std::sync::Arc;

pub struct ReadConsolidationView {
    store: Arc<dyn ConsolidationStore>,
}
impl ReadConsolidationView {
    pub fn new(store: Arc<dyn ConsolidationStore>) -> Self {
        Self { store }
    }
    pub async fn execute(
        &self,
        about: String,
        view: String,
        revision: Option<u64>,
    ) -> Result<ConsolidationRead, PortError> {
        self.store.read_consolidation(about, view, revision).await
    }
}
