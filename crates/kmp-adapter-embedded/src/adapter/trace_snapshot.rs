use super::{
    bounded_adjacency,
    engine::{Key, ReadTx, Table},
    serdes::{NodeRecord, decode},
};
use kmp_domain::{AdjacencyPage, AdjacencyRequest, NodeProjection, PortError, TraceSnapshotReader};

pub(super) struct TraceSnapshot<'a>(pub &'a dyn ReadTx);

impl TraceSnapshotReader for TraceSnapshot<'_> {
    fn bodies(
        &self,
        ids: &[String],
    ) -> Result<Vec<Option<kmp_domain::NodeDetailProjection>>, PortError> {
        super::node_detail::read_batch(self.0, ids)
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
}

#[cfg(test)]
#[path = "trace_snapshot_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "trace_proof_snapshot_tests.rs"]
mod proof_tests;
