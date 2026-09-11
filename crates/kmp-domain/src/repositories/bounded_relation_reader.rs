use std::{future::Future, sync::Arc};

use crate::{AdjacencyPage, AdjacencyRequest, PortError};

/// Each call reads one bounded page from one snapshot. The caller must bind
/// multiple calls to a revision or use the adapter's transaction-local reader.
pub trait BoundedRelationReader {
    fn read_adjacency(
        &self,
        request: &AdjacencyRequest,
    ) -> impl Future<Output = Result<AdjacencyPage, PortError>> + Send;
}

impl<T: BoundedRelationReader + Send + Sync + ?Sized> BoundedRelationReader for Arc<T> {
    async fn read_adjacency(&self, request: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        self.as_ref().read_adjacency(request).await
    }
}

impl<T: BoundedRelationReader + Send + Sync + ?Sized> BoundedRelationReader for &T {
    async fn read_adjacency(&self, request: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        (*self).read_adjacency(request).await
    }
}
