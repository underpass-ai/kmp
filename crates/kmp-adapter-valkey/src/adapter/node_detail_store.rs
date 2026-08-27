use kmp_ports::{
    NodeDetailProjection, NodeDetailReader, PortError, ProjectionMutation, ProjectionWriter,
};

use crate::adapter::endpoint::{DEFAULT_NODE_DETAIL_KEY_PREFIX, ValkeyEndpoint};
use crate::adapter::io::{execute_get_command, execute_mget_command, execute_set_command};
use crate::adapter::node_detail_serialization::{deserialize_node_detail, serialize_node_detail};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValkeyNodeDetailStore {
    pub(crate) endpoint: ValkeyEndpoint,
}

impl ValkeyNodeDetailStore {
    pub fn new(detail_uri: impl Into<String>) -> Result<Self, PortError> {
        let endpoint = ValkeyEndpoint::parse_with_default_key_prefix(
            detail_uri.into(),
            "detail",
            DEFAULT_NODE_DETAIL_KEY_PREFIX,
        )?;
        Ok(Self { endpoint })
    }

    pub(crate) fn detail_key(&self, node_id: &str) -> String {
        format!("{}:{}", self.endpoint.key_prefix, node_id)
    }

    pub(crate) fn detail_payload(
        &self,
        detail: &NodeDetailProjection,
    ) -> Result<String, PortError> {
        serialize_node_detail(detail)
    }

    async fn execute_set_command(&self, key: &str, payload: &str) -> Result<(), PortError> {
        execute_set_command(&self.endpoint, key, payload, None).await
    }

    async fn execute_get_command(&self, key: &str) -> Result<Option<String>, PortError> {
        execute_get_command(&self.endpoint, key).await
    }
}

impl ProjectionWriter for ValkeyNodeDetailStore {
    async fn apply_mutations(&self, mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        for mutation in mutations {
            match mutation {
                ProjectionMutation::UpsertNodeDetail(detail) => {
                    let key = self.detail_key(&detail.node_id);
                    let payload = self.detail_payload(&detail)?;
                    self.execute_set_command(&key, &payload).await?;
                }
                ProjectionMutation::EnsureNode(node) => {
                    return Err(PortError::InvalidState(format!(
                        "valkey detail store does not persist graph node `{}`",
                        node.node_id
                    )));
                }
                ProjectionMutation::UpsertNode(node) => {
                    return Err(PortError::InvalidState(format!(
                        "valkey detail store does not persist graph node `{}`",
                        node.node_id
                    )));
                }
                ProjectionMutation::UpdateNodeStatus { node_id, .. } => {
                    return Err(PortError::InvalidState(format!(
                        "valkey detail store does not persist graph node `{node_id}`"
                    )));
                }
                ProjectionMutation::UpsertNodeRelation(relation) => {
                    return Err(PortError::InvalidState(format!(
                        "valkey detail store does not persist graph relation `{} -> {}`",
                        relation.source_node_id, relation.target_node_id
                    )));
                }
            }
        }

        Ok(())
    }
}

impl NodeDetailReader for ValkeyNodeDetailStore {
    async fn load_node_detail(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeDetailProjection>, PortError> {
        let key = self.detail_key(node_id);
        match self.execute_get_command(&key).await? {
            Some(payload) => deserialize_node_detail(&payload),
            None => Ok(None),
        }
    }

    async fn load_node_details_batch(
        &self,
        node_ids: Vec<String>,
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }
        let keys: Vec<String> = node_ids.iter().map(|id| self.detail_key(id)).collect();
        let payloads = execute_mget_command(&self.endpoint, &keys).await?;
        payloads
            .into_iter()
            .map(|payload| match payload {
                Some(p) => deserialize_node_detail(&p),
                None => Ok(None),
            })
            .collect()
    }
}
