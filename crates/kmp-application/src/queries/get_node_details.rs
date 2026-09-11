use kmp_domain::{GraphNeighborhoodReader, NodeDetailReader, PortError};

use super::get_node_detail::{map_node, map_node_detail, trim_to_option};
use super::{GetNodeDetailResult, QueryApplicationService};
use crate::ApplicationError;

impl<G, D, S> QueryApplicationService<G, D, S>
where
    G: GraphNeighborhoodReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
{
    /// Materialize already selected refs in order, retaining absent nodes and
    /// absent bodies as different states. Use this service's operation snapshot
    /// when graph and bodies must describe the same committed state.
    pub async fn get_node_details(
        &self,
        node_ids: Vec<String>,
    ) -> Result<Vec<Option<GetNodeDetailResult>>, ApplicationError> {
        let ids = node_ids
            .iter()
            .map(|id| {
                trim_to_option(id)
                    .ok_or_else(|| ApplicationError::Validation("node_id cannot be empty".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let nodes = self.graph_reader.load_nodes_batch(ids.clone()).await?;
        if nodes.len() != ids.len()
            || nodes
                .iter()
                .zip(&ids)
                .any(|(node, id)| node.as_ref().is_some_and(|node| node.node_id != *id))
        {
            return Err(invalid_batch("graph"));
        }
        // A dangling link is not a source. Do not read orphaned bodies whose
        // graph node is absent; the single-node operation skips them too.
        let present: Vec<_> = nodes
            .iter()
            .flatten()
            .map(|node| node.node_id.clone())
            .collect();
        let details = if present.is_empty() {
            Vec::new()
        } else {
            self.detail_reader
                .load_node_details_batch(present.clone())
                .await?
        };
        if details.len() != present.len()
            || details
                .iter()
                .zip(&present)
                .any(|(detail, id)| detail.as_ref().is_some_and(|detail| detail.node_id != *id))
        {
            return Err(invalid_batch("detail"));
        }
        let mut details = details.into_iter();
        Ok(nodes
            .into_iter()
            .map(|node| {
                node.map(|node| GetNodeDetailResult {
                    node: map_node(&node),
                    detail: details
                        .next()
                        .expect("validated batch length")
                        .map(map_node_detail),
                })
            })
            .collect())
    }
}

fn invalid_batch(port: &str) -> ApplicationError {
    PortError::InvalidState(format!(
        "{port} batch does not preserve requested node slots"
    ))
    .into()
}
