use crate::ApplicationError;
use crate::queries::QueryApplicationService;
use kmp_domain::{GraphNeighborhoodReader, KmpBundle};

impl<G, D, S> QueryApplicationService<G, D, S>
where
    G: GraphNeighborhoodReader + Send + Sync,
{
    /// Replace admitted graph headers with their canonical node projections.
    /// Absence at this point contradicts the selection's pinned catalogue.
    pub(crate) async fn materialize_selected_nodes(
        &self,
        bundle: KmpBundle,
        selected: &std::collections::BTreeSet<String>,
    ) -> Result<KmpBundle, ApplicationError> {
        let ids = selected.iter().cloned().collect::<Vec<_>>();
        let mut nodes = std::collections::BTreeMap::new();
        if !ids.is_empty() {
            let loaded = self.graph_reader.load_nodes_batch(ids.clone()).await?;
            if loaded.len() != ids.len() {
                return Err(ApplicationError::Ports(
                    kmp_domain::PortError::InvalidState(
                        "selected node batch did not preserve its slots".into(),
                    ),
                ));
            }
            for (id, node) in ids.into_iter().zip(loaded) {
                let node = node.filter(|node| node.node_id == id).ok_or_else(|| {
                    ApplicationError::Ports(kmp_domain::PortError::Conflict(format!(
                        "selected node `{id}` is no longer present in its catalogue snapshot",
                    )))
                })?;
                nodes.insert(id, kmp_domain::BundleNode::from_projection(&node));
            }
        }
        let root = nodes
            .remove(bundle.root_node().node_id())
            .unwrap_or_else(|| bundle.root_node().clone());
        let neighbors = bundle
            .neighbor_nodes()
            .iter()
            .map(|node| nodes.remove(node.node_id()).unwrap_or_else(|| node.clone()))
            .collect();
        Ok(KmpBundle::new(
            bundle.root_node_id().clone(),
            bundle.role().clone(),
            root,
            neighbors,
            bundle.relationships().to_vec(),
            bundle.node_details().to_vec(),
            bundle.metadata().clone(),
        )?)
    }
}
