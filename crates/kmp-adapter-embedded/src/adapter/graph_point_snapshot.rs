use kmp_domain::{
    AdjacencyRequest, ContextUpdatedEvent, GraphPointReads, NodeDetailProjection, NodeProjection,
    NodeRelationProjection, PortError, RelationDirection,
};

use super::bounded_adjacency;
use super::engine::{Key, ReadTx, Table};
use super::serdes::{DetailRecord, NodeRecord, decode, decode_explanation};
use super::store::EmbeddedKernelStore;

/// Rows read per adjacency page while a whole adjacency is collected.
const PAGE: u32 = 1024;

/// [`GraphPointReads`] over one read transaction: every answer comes from
/// the same snapshot.
struct GraphPointSnapshot<'a>(&'a dyn ReadTx);

impl GraphPointSnapshot<'_> {
    fn adjacency(
        &self,
        node_id: &str,
        direction: RelationDirection,
        relation_type: Option<&str>,
    ) -> Result<Vec<NodeRelationProjection>, PortError> {
        let mut request = AdjacencyRequest::new(node_id, direction, PAGE)
            .map_err(|error| PortError::InvalidState(error.to_string()))?;
        if let Some(relation_type) = relation_type {
            request = request
                .with_relation_type(relation_type)
                .map_err(|error| PortError::InvalidState(error.to_string()))?;
        }
        let mut edges = Vec::new();
        loop {
            let page = bounded_adjacency::read_page(self.0, &request)?;
            edges.extend(page.edges);
            match page.next {
                Some(position) if !page.exhausted => {
                    request = request.with_after(position);
                }
                _ => return Ok(edges),
            }
        }
    }
}

impl GraphPointReads for GraphPointSnapshot<'_> {
    fn last_event_sequence(&self) -> Result<u64, PortError> {
        Ok(self
            .0
            .last_u64(Table::EventLog)?
            .map_or(0, |(sequence, _)| sequence))
    }

    fn event(&self, sequence: u64) -> Result<Option<ContextUpdatedEvent>, PortError> {
        self.0
            .get(Table::EventLog, Key::U64(sequence))?
            .map(|raw| decode("context event", &raw))
            .transpose()
    }

    fn node(&self, node_id: &str) -> Result<Option<NodeProjection>, PortError> {
        self.0
            .get(Table::Nodes, Key::Str(node_id))?
            .map(|raw| decode::<NodeRecord>("graph node", &raw)?.into_projection())
            .transpose()
    }

    fn detail(&self, node_id: &str) -> Result<Option<NodeDetailProjection>, PortError> {
        self.0
            .get(Table::Details, Key::Str(node_id))?
            .map(|raw| decode::<DetailRecord>("node detail", &raw).map(Into::into))
            .transpose()
    }

    fn relation(
        &self,
        source: &str,
        target: &str,
        relation_type: &str,
    ) -> Result<Option<NodeRelationProjection>, PortError> {
        self.0
            .get(Table::Relations, Key::Str3(source, target, relation_type))?
            .map(|raw| {
                Ok(NodeRelationProjection {
                    source_node_id: source.to_string(),
                    target_node_id: target.to_string(),
                    relation_type: relation_type.to_string(),
                    explanation: decode_explanation(&raw)?,
                })
            })
            .transpose()
    }

    fn outgoing(
        &self,
        node_id: &str,
        relation_type: Option<&str>,
    ) -> Result<Vec<NodeRelationProjection>, PortError> {
        self.adjacency(node_id, RelationDirection::Outgoing, relation_type)
    }

    fn incoming(
        &self,
        node_id: &str,
        relation_type: Option<&str>,
    ) -> Result<Vec<NodeRelationProjection>, PortError> {
        self.adjacency(node_id, RelationDirection::Incoming, relation_type)
    }

    fn outgoing_count(&self, node_id: &str, relation_type: &str) -> Result<u64, PortError> {
        self.0
            .count_str3_of_kind(Table::Relations, node_id, relation_type)
    }
}

impl EmbeddedKernelStore {
    /// Runs `task` over point reads of one consistent snapshot of the graph
    /// and its event log, on a blocking worker. What a derived index beside
    /// the store follows the log with.
    pub async fn read_points<T, F>(&self, task: F) -> Result<T, PortError>
    where
        T: Send + 'static,
        F: FnOnce(&dyn GraphPointReads) -> Result<T, PortError> + Send + 'static,
    {
        self.run(move |store| {
            let tx = store.begin_read()?;
            task(&GraphPointSnapshot(tx.as_ref()))
        })
        .await
    }
}

#[cfg(test)]
#[path = "graph_point_snapshot_tests.rs"]
mod tests;
