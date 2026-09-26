use kmp_domain::{AdjacencyLifecycleLinks, LifecycleChain, LifecycleChainReader, PortError};

use super::{bounded_adjacency, store::EmbeddedKernelStore};

/// The lifecycle around a memory, walked inside one read transaction over
/// the `relations_*_by_kind` index: each hop is one typed range per
/// lifecycle relation, never a scan of the memory's other relations.
impl LifecycleChainReader for EmbeddedKernelStore {
    async fn read_lifecycle_chain(&self, from: &str) -> Result<LifecycleChain, PortError> {
        let from = from.to_string();
        self.run(move |store| {
            let tx = store.begin_read()?;
            let links = AdjacencyLifecycleLinks::new(|request| {
                bounded_adjacency::read_page(tx.as_ref(), request)
            });
            LifecycleChain::walk(&from, &links)
        })
        .await
    }
}
