use super::temporal_index_identity::TemporalIndexIdentity;
use super::*;
use kmp_domain::TemporalMemoryIndex;

impl<G, D, S, E, W> KernelMemoryApplicationService<G, D, S, E, W>
where
    G: GraphNeighborhoodReader + MemoryAboutIndexReader + NodeRelationshipReader + Send + Sync,
    D: NodeDetailReader + Send + Sync,
    S: SnapshotStore + Send + Sync,
    E: ContextEventStore + Send + Sync,
    W: ProjectionWriter + Send + Sync,
{
    pub(super) async fn temporal_index_read(
        &self,
        query: &TemporalMemoryQuery,
    ) -> Result<(Arc<TemporalMemoryIndex>, DimensionSelection), ApplicationError> {
        let dimensions = query.dimensions.resolve_current_about(&query.about);
        let roots = self.memory_context_roots(&query.about, &dimensions).await?;
        let depth = crate::queries::clamp_native_graph_traversal_depth(query.depth);
        let identity = self
            .query_application
            .graph_read_revision()
            .await?
            .map(|revision| TemporalIndexIdentity::new(revision, roots.clone(), depth, query.axis));
        if let Some(identity) = &identity {
            let cache = self
                .temporal_index_cache
                .lock()
                .map_err(|_| cache_error())?;
            if let Some(index) = cache.get(identity) {
                return Ok((index, dimensions));
            }
        }
        let mut catalogues = Vec::with_capacity(roots.len());
        for root in &roots {
            catalogues.push(
                self.query_application
                    .read_context_catalogue(
                        &kmp_domain::NeighborhoodRequest::new(root, depth),
                        "temporal-reader",
                    )
                    .await?,
            );
        }
        let bundle = crate::memory::merge_memory_bundles::merge(catalogues)?;
        let bundle = super::temporal_catalogue::admission_catalogue(bundle)?;
        let index = Arc::new(TemporalMemoryIndex::new(bundle, query.axis)?);
        if let Some(identity) = identity {
            self.temporal_index_cache
                .lock()
                .map_err(|_| cache_error())?
                .put(identity, Arc::clone(&index));
        }
        Ok((index, dimensions))
    }
}

fn cache_error() -> ApplicationError {
    ApplicationError::Ports(kmp_domain::PortError::Unavailable(
        "temporal index cache lock poisoned".into(),
    ))
}
