use std::future::Future;
use std::sync::Arc;

use crate::{ContextPathNeighborhood, NodeNeighborhood, PortError};

pub trait GraphNeighborhoodReader {
    fn load_neighborhood(
        &self,
        root_node_id: &str,
        depth: u32,
    ) -> impl Future<Output = Result<Option<NodeNeighborhood>, PortError>> + Send;

    fn load_context_path(
        &self,
        root_node_id: &str,
        target_node_id: &str,
        subtree_depth: u32,
    ) -> impl Future<Output = Result<Option<ContextPathNeighborhood>, PortError>> + Send;
}

impl<T> GraphNeighborhoodReader for Arc<T>
where
    T: GraphNeighborhoodReader + Send + Sync + ?Sized,
{
    async fn load_neighborhood(
        &self,
        root_node_id: &str,
        depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        self.as_ref().load_neighborhood(root_node_id, depth).await
    }

    async fn load_context_path(
        &self,
        root_node_id: &str,
        target_node_id: &str,
        subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        self.as_ref()
            .load_context_path(root_node_id, target_node_id, subtree_depth)
            .await
    }
}

impl<T> GraphNeighborhoodReader for &T
where
    T: GraphNeighborhoodReader + Send + Sync + ?Sized,
{
    async fn load_neighborhood(
        &self,
        root_node_id: &str,
        depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        (*self).load_neighborhood(root_node_id, depth).await
    }

    async fn load_context_path(
        &self,
        root_node_id: &str,
        target_node_id: &str,
        subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        (*self)
            .load_context_path(root_node_id, target_node_id, subtree_depth)
            .await
    }
}
