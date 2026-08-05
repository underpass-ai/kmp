use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rehydration_domain::{
    ContextEventStore, ContextPathNeighborhood, ContextUpdatedEvent, GraphNeighborhoodReader,
    IdempotentOutcome, MemoryAboutIndexReader, MemoryDimensionIdentity, NodeDetailProjection,
    NodeDetailReader, NodeNeighborhood, NodeRelationshipReader, NodeRelationships, PortError,
    ProcessedEventStore, ProjectionCheckpoint, ProjectionCheckpointStore, ProjectionMutation,
    ProjectionWriter, RehydrationBundle, SnapshotSaveOptions, SnapshotStore,
};
use tokio::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct InMemoryGraphNeighborhoodReader {
    neighborhoods: HashMap<String, NodeNeighborhood>,
}

impl InMemoryGraphNeighborhoodReader {
    pub fn with_neighborhood(neighborhood: NodeNeighborhood) -> Self {
        let mut neighborhoods = HashMap::new();
        neighborhoods.insert(neighborhood.root.node_id.clone(), neighborhood);
        Self { neighborhoods }
    }
}

impl GraphNeighborhoodReader for InMemoryGraphNeighborhoodReader {
    async fn load_neighborhood(
        &self,
        root_node_id: &str,
        _depth: u32,
    ) -> Result<Option<NodeNeighborhood>, PortError> {
        Ok(self.neighborhoods.get(root_node_id).cloned())
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

impl MemoryAboutIndexReader for InMemoryGraphNeighborhoodReader {
    async fn list_memory_abouts(&self) -> Result<Vec<String>, PortError> {
        let mut abouts = self.neighborhoods.keys().cloned().collect::<Vec<_>>();
        abouts.sort();
        Ok(abouts)
    }

    async fn list_memory_abouts_by_dimensions(
        &self,
        dimension_ids: &[String],
    ) -> Result<Vec<String>, PortError> {
        let wanted = dimension_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let mut abouts = self
            .neighborhoods
            .iter()
            .filter(|(_, neighborhood)| {
                neighborhood.relations.iter().any(|relation| {
                    relation.relation_type == "has_dimension"
                        && dimension_matches(&relation.target_node_id, &wanted)
                })
            })
            .map(|(about, _)| about.clone())
            .collect::<Vec<_>>();
        abouts.sort();
        Ok(abouts)
    }
}

fn dimension_matches(node_id: &str, wanted: &HashSet<&str>) -> bool {
    wanted.contains(node_id)
        || MemoryDimensionIdentity::parse(node_id)
            .map(|identity| wanted.contains(identity.dimension_id()))
            .unwrap_or(false)
}

impl NodeRelationshipReader for InMemoryGraphNeighborhoodReader {
    async fn load_node_relationships(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeRelationships>, PortError> {
        let Some(neighborhood) = self.neighborhoods.get(node_id) else {
            return Ok(None);
        };
        Ok(Some(NodeRelationships {
            incoming: neighborhood
                .relations
                .iter()
                .filter(|relation| relation.target_node_id == node_id)
                .cloned()
                .collect(),
            outgoing: neighborhood
                .relations
                .iter()
                .filter(|relation| relation.source_node_id == node_id)
                .cloned()
                .collect(),
        }))
    }
}

impl ProjectionWriter for InMemoryGraphNeighborhoodReader {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryNodeDetailReader {
    details: HashMap<String, NodeDetailProjection>,
}

impl InMemoryNodeDetailReader {
    pub fn with_details(details: impl IntoIterator<Item = NodeDetailProjection>) -> Self {
        Self {
            details: details
                .into_iter()
                .map(|detail| (detail.node_id.clone(), detail))
                .collect(),
        }
    }
}

impl NodeDetailReader for InMemoryNodeDetailReader {
    async fn load_node_detail(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeDetailProjection>, PortError> {
        Ok(self.details.get(node_id).cloned())
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

impl ProjectionWriter for InMemoryNodeDetailReader {
    async fn apply_mutations(&self, _mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryProjectionWriter {
    mutations: Arc<Mutex<Vec<ProjectionMutation>>>,
}

impl InMemoryProjectionWriter {
    pub async fn mutations(&self) -> Vec<ProjectionMutation> {
        self.mutations.lock().await.clone()
    }
}

impl ProjectionWriter for InMemoryProjectionWriter {
    async fn apply_mutations(&self, mutations: Vec<ProjectionMutation>) -> Result<(), PortError> {
        self.mutations.lock().await.extend(mutations);
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryProcessedEventStore {
    processed: Arc<Mutex<HashSet<(String, String)>>>,
}

impl InMemoryProcessedEventStore {
    pub async fn processed(&self) -> HashSet<(String, String)> {
        self.processed.lock().await.clone()
    }
}

impl ProcessedEventStore for InMemoryProcessedEventStore {
    async fn has_processed(&self, consumer_name: &str, event_id: &str) -> Result<bool, PortError> {
        Ok(self
            .processed
            .lock()
            .await
            .contains(&(consumer_name.to_string(), event_id.to_string())))
    }

    async fn record_processed(&self, consumer_name: &str, event_id: &str) -> Result<(), PortError> {
        self.processed
            .lock()
            .await
            .insert((consumer_name.to_string(), event_id.to_string()));
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryProjectionCheckpointStore {
    checkpoints: Arc<Mutex<HashMap<(String, String), ProjectionCheckpoint>>>,
}

impl InMemoryProjectionCheckpointStore {
    pub async fn checkpoint(
        &self,
        consumer_name: &str,
        stream_name: &str,
    ) -> Option<ProjectionCheckpoint> {
        self.checkpoints
            .lock()
            .await
            .get(&(consumer_name.to_string(), stream_name.to_string()))
            .cloned()
    }
}

impl ProjectionCheckpointStore for InMemoryProjectionCheckpointStore {
    async fn load_checkpoint(
        &self,
        consumer_name: &str,
        stream_name: &str,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        Ok(self
            .checkpoints
            .lock()
            .await
            .get(&(consumer_name.to_string(), stream_name.to_string()))
            .cloned())
    }

    async fn save_checkpoint(&self, checkpoint: ProjectionCheckpoint) -> Result<(), PortError> {
        let key = (
            checkpoint.consumer_name.clone(),
            checkpoint.stream_name.clone(),
        );
        self.checkpoints.lock().await.insert(key, checkpoint);
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSnapshotStore;

impl SnapshotStore for NoopSnapshotStore {
    async fn save_bundle_with_options(
        &self,
        _bundle: &RehydrationBundle,
        _options: SnapshotSaveOptions,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

pub struct InMemoryContextEventStore {
    revisions: Mutex<HashMap<String, u64>>,
    hashes: Mutex<HashMap<String, String>>,
    idempotency: Mutex<HashMap<String, IdempotentOutcome>>,
}

impl InMemoryContextEventStore {
    pub fn new() -> Self {
        Self {
            revisions: Mutex::new(HashMap::new()),
            hashes: Mutex::new(HashMap::new()),
            idempotency: Mutex::new(HashMap::new()),
        }
    }

    fn aggregate_key(root_node_id: &str, role: &str) -> String {
        format!("{root_node_id}:{role}")
    }
}

impl Default for InMemoryContextEventStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextEventStore for InMemoryContextEventStore {
    async fn append(
        &self,
        event: ContextUpdatedEvent,
        expected_revision: u64,
    ) -> Result<u64, PortError> {
        let key = Self::aggregate_key(&event.root_node_id, &event.role);
        let mut revisions = self.revisions.lock().await;
        let current = revisions.get(&key).copied().unwrap_or(0);
        if current != expected_revision {
            return Err(PortError::Conflict(format!(
                "expected revision {expected_revision}, current is {current}"
            )));
        }
        let new_revision = current + 1;
        revisions.insert(key.clone(), new_revision);
        self.hashes
            .lock()
            .await
            .insert(key, event.content_hash.clone());
        if let Some(ref idem_key) = event.idempotency_key {
            self.idempotency.lock().await.insert(
                idem_key.clone(),
                IdempotentOutcome {
                    revision: new_revision,
                    content_hash: event.content_hash,
                    logical_digest: event.logical_digest,
                },
            );
        }
        Ok(new_revision)
    }

    async fn current_revision(&self, root_node_id: &str, role: &str) -> Result<u64, PortError> {
        let key = Self::aggregate_key(root_node_id, role);
        Ok(self.revisions.lock().await.get(&key).copied().unwrap_or(0))
    }

    async fn current_content_hash(
        &self,
        root_node_id: &str,
        role: &str,
    ) -> Result<Option<String>, PortError> {
        let key = Self::aggregate_key(root_node_id, role);
        Ok(self.hashes.lock().await.get(&key).cloned())
    }

    async fn find_by_idempotency_key(
        &self,
        key: &str,
    ) -> Result<Option<IdempotentOutcome>, PortError> {
        Ok(self.idempotency.lock().await.get(key).cloned())
    }
}
