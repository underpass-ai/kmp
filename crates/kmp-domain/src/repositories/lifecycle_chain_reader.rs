use std::{future::Future, sync::Arc};

use crate::{LifecycleChain, PortError};

/// Reads the declared lifecycle around one memory from one snapshot.
///
/// A store answers from its typed relation index, one bounded range per hop
/// and relation type ([`crate::AdjacencyLifecycleLinks`]).
pub trait LifecycleChainReader {
    fn read_lifecycle_chain(
        &self,
        from: &str,
    ) -> impl Future<Output = Result<LifecycleChain, PortError>> + Send;
}

impl<T: LifecycleChainReader + Send + Sync + ?Sized> LifecycleChainReader for Arc<T> {
    async fn read_lifecycle_chain(&self, from: &str) -> Result<LifecycleChain, PortError> {
        self.as_ref().read_lifecycle_chain(from).await
    }
}
