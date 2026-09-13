use super::*;

impl<G, D, S, E, W> KernelMemoryApplicationService<G, D, S, E, W>
where
    G: GraphNeighborhoodReader + MemoryAboutIndexReader + NodeRelationshipReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
    E: ContextEventStore + Send + Sync,
    W: ProjectionWriter + Send + Sync,
{
    pub(super) async fn materialize_temporal_supports(
        &self,
        bundle: KmpBundle,
        selected: &BTreeSet<String>,
    ) -> Result<KmpBundle, ApplicationError> {
        let sources = std::iter::once(bundle.root_node())
            .chain(bundle.neighbor_nodes())
            .filter(|node| {
                selected.contains(node.node_id())
                    && matches!(node.node_kind(), "memory_evidence" | "evidence")
            })
            .map(|node| node.node_id().to_string())
            .collect::<BTreeSet<_>>();
        let mut canonical = BTreeMap::new();
        for source in &sources {
            let links = self
                .query_application
                .get_node_relationships(GetNodeRelationshipsQuery {
                    node_id: source.clone(),
                })
                .await?;
            for edge in links
                .outgoing
                .into_iter()
                .filter(|edge| edge.relationship_type == "supports")
            {
                canonical.insert((edge.source_node_id, edge.target_node_id), edge.explanation);
            }
        }
        let mut relationships = Vec::with_capacity(bundle.relationships().len());
        for edge in bundle.relationships() {
            if edge.relationship_type() == "supports" && sources.contains(edge.source_node_id()) {
                let explanation = canonical
                    .get(&(
                        edge.source_node_id().to_string(),
                        edge.target_node_id().to_string(),
                    ))
                    .ok_or_else(|| {
                        ApplicationError::Ports(kmp_domain::PortError::Conflict(
                            "selected support changed inside its catalogue read".into(),
                        ))
                    })?;
                let mut restored = BundleRelationship::new(
                    edge.source_node_id(),
                    edge.target_node_id(),
                    edge.relationship_type(),
                    explanation.clone(),
                );
                if let Some(provenance) = edge.provenance() {
                    restored = restored.with_provenance(provenance.clone());
                }
                relationships.push(restored);
            } else {
                relationships.push(edge.clone());
            }
        }
        Ok(KmpBundle::new(
            bundle.root_node_id().clone(),
            bundle.role().clone(),
            bundle.root_node().clone(),
            bundle.neighbor_nodes().to_vec(),
            relationships,
            bundle.node_details().to_vec(),
            bundle.metadata().clone(),
        )?)
    }
}
