use kmp_domain::{
    ContextPathNeighborhood, GraphNeighborhoodReader, KmpBundle, MemoryAboutIndexReader,
    NodeDetailProjection, NodeDetailReader, NodeNeighborhood, NodeRelationshipReader,
    NodeRelationships, PortError, ProjectionMutation, ProjectionWriter, SnapshotSaveOptions,
    SnapshotStore,
};

pub struct EmptyGraphNeighborhoodReader;

impl GraphNeighborhoodReader for EmptyGraphNeighborhoodReader {
    async fn load_neighborhood(
        &self,
        _root_node_id: &str,
        _depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        Ok(None)
    }

    async fn load_context_path(
        &self,
        _root_node_id: &str,
        _target_node_id: &str,
        _subtree_depth: u32,
    ) -> Result<Option<ContextPathNeighborhood>, PortError> {
        Ok(None)
    }
}

impl NodeRelationshipReader for EmptyGraphNeighborhoodReader {
    async fn load_node_relationships(
        &self,
        _node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        Ok(None)
    }
}

impl MemoryAboutIndexReader for EmptyGraphNeighborhoodReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        Ok(Vec::new())
    }
}

impl ProjectionWriter for EmptyGraphNeighborhoodReader {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

pub struct EmptyNodeDetailReader;

impl NodeDetailReader for EmptyNodeDetailReader {
    async fn load_node_detail(
        &self,
        _node_id: &str,
    ) -> Result<Option<NodeDetailProjection>, PortError> {
        Ok(None)
    }

    async fn load_node_details_batch(
        &self,
        node_ids: Vec<String>,
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        let mut results = Vec::with_capacity(node_ids.len());
        for node_id in &node_ids {
            results.push(self.load_node_detail(node_id).await?);
        }
        Ok(results)
    }
}

impl ProjectionWriter for EmptyNodeDetailReader {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

pub struct NoopSnapshotStore;

impl SnapshotStore for NoopSnapshotStore {
    async fn save_bundle_with_options(
        &self,
        _bundle: &KmpBundle,
        _options: SnapshotSaveOptions,
    ) -> Result<(), PortError> {
        Ok(())
    }
}
