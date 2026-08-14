use std::future::Future;
use std::sync::Arc;

use crate::{NodeRelationProjection, PortError};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeRelationships {
    pub incoming: Vec<NodeRelationProjection>,
    pub outgoing: Vec<NodeRelationProjection>,
}

pub trait NodeRelationshipReader {
    fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> impl Future<Output = Result<Option<NodeRelationships>, PortError>> + Send;
}

impl<T> NodeRelationshipReader for Arc<T>
where
    T: NodeRelationshipReader + Send + Sync + ?Sized,
{
    async fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        self.as_ref().load_node_relationships(node_id).await
    }
}

impl<T> NodeRelationshipReader for &T
where
    T: NodeRelationshipReader + Send + Sync + ?Sized,
{
    async fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        (*self).load_node_relationships(node_id).await
    }
}
