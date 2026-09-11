use std::sync::Arc;

#[derive(Debug)]
pub struct QueryApplicationService<G, D, S> {
    pub(crate) graph_reader: Arc<G>,
    pub(crate) detail_reader: Arc<D>,
    pub(crate) snapshot_store: Arc<S>,
    pub(crate) generator_version: &'static str,
    read_snapshot_provider: Option<Arc<dyn kmp_domain::ReadSnapshotProvider<G, D>>>,
}

impl<G, D, S> QueryApplicationService<G, D, S> {
    pub fn new(
        graph_reader: Arc<G>,
        detail_reader: Arc<D>,
        snapshot_store: Arc<S>,
        generator_version: &'static str,
    ) -> Self {
        Self {
            graph_reader,
            detail_reader,
            snapshot_store,
            generator_version,
            read_snapshot_provider: None,
        }
    }
    /// Configure a backend that owns graph and bodies in one store. Without
    /// this provider separate adapters retain their best-effort read semantics.
    /// A configured provider error propagates; it never falls back to live reads.
    pub fn with_read_snapshots(
        mut self,
        provider: Arc<dyn kmp_domain::ReadSnapshotProvider<G, D>>,
    ) -> Self {
        self.read_snapshot_provider = Some(provider);
        self
    }

    pub async fn read_snapshot(&self) -> Result<Option<Self>, crate::ApplicationError> {
        let Some(provider) = &self.read_snapshot_provider else {
            return Ok(None);
        };
        let (graph, detail) = provider.open_snapshot().await?;
        Ok(Some(Self::new(
            Arc::new(graph),
            Arc::new(detail),
            Arc::clone(&self.snapshot_store),
            self.generator_version,
        )))
    }
}
