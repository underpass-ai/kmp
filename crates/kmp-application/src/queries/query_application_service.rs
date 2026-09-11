use std::sync::Arc;

#[derive(Debug)]
pub struct QueryApplicationService<G, D, S> {
    pub(crate) graph_reader: Arc<G>,
    pub(crate) detail_reader: Arc<D>,
    pub(crate) snapshot_store: Arc<S>,
    pub(crate) generator_version: &'static str,
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
        }
    }
}

impl<G: kmp_domain::GraphNeighborhoodReader + Send + Sync, D, S> QueryApplicationService<G, D, S> {
    pub async fn trace_search(
        &self,
        request: &kmp_domain::TraceSearchRequest,
    ) -> Result<kmp_domain::TraceSearchResult, crate::ApplicationError> {
        Ok(self.graph_reader.load_bounded_trace(request).await?)
    }
}
