use super::{
    bounded_adjacency,
    engine::{Key, ReadTx, Table},
    serdes::{NodeRecord, decode},
};
use kmp_domain::{
    AdjacencyPage, AdjacencyRequest, NodeProjection, PortError, RelationDirection,
    TraceSnapshotReader,
};

pub(super) struct TraceSnapshot<'a>(pub &'a dyn ReadTx);

impl TraceSnapshotReader for TraceSnapshot<'_> {
    fn bodies(
        &self,
        ids: &[String],
    ) -> Result<Vec<Option<kmp_domain::NodeDetailProjection>>, PortError> {
        super::node_detail::read_batch(self.0, ids)
    }

    fn verified_bodies(
        &self,
        ids: &[String],
    ) -> Result<Vec<Option<kmp_domain::NodeDetailProjection>>, PortError> {
        super::node_body_descriptor::read_verified_bodies(self.0, ids)
    }

    fn descriptors(
        &self,
        ids: &[String],
    ) -> Result<Vec<Option<kmp_domain::NodeBodyDescriptor>>, PortError> {
        super::node_body_descriptor::read_batch(self.0, ids)
    }

    fn cards(
        &self,
        ids: &[String],
        language: &str,
    ) -> Result<Vec<Option<kmp_domain::NodeCard>>, PortError> {
        super::node_card::read_batch(self.0, ids, language)
    }

    fn cards_at(
        &self,
        ids: &[String],
        language: &str,
        cut_nanos: Option<i128>,
    ) -> Result<Vec<Option<kmp_domain::NodeCard>>, PortError> {
        super::node_card::read_batch_at(self.0, ids, language, cut_nanos)
    }

    fn node(&self, id: &str) -> Result<Option<NodeProjection>, PortError> {
        self.0
            .get(Table::Nodes, Key::Str(id))?
            .map(|raw| decode::<NodeRecord>("trace node", &raw)?.into_projection())
            .transpose()
    }

    fn adjacency(&self, request: &AdjacencyRequest) -> Result<AdjacencyPage, PortError> {
        bounded_adjacency::read_page(self.0, request)
    }

    /// Keys only: incoming rows skip the point read of each explanation.
    fn neighbor_ids(
        &self,
        node: &str,
        direction: RelationDirection,
        limit: u32,
    ) -> Result<Vec<String>, PortError> {
        let table = match direction {
            RelationDirection::Outgoing => Table::Relations,
            RelationDirection::Incoming => Table::RelationsByTarget,
        };
        Ok(self
            .0
            .scan_str3_page(table, node, None, limit, None)?
            .into_iter()
            .map(|((_, neighbor, _), _)| neighbor)
            .collect())
    }
}

#[cfg(test)]
#[path = "trace_snapshot_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "trace_proof_snapshot_tests.rs"]
mod proof_tests;
