use crate::{AdjacencyPage, AdjacencyRequest, PortError, RelationDirection};

use super::{
    LifecycleLink, LifecycleLinkSource, LifecycleNeighbours, LifecycleRelation,
    MAX_LIFECYCLE_LINKS_PER_NODE,
};

/// Lifecycle links read from a store's typed adjacency.
///
/// Each read is one bounded range per relation type: incoming rows of the
/// type for what replaced a memory, outgoing rows for what it replaced. On
/// the embedded store that is the `relations_*_by_kind(k1, k3, k2)` index,
/// so one hop costs O(log N) whatever the store's size, and never a scan of
/// the memory's other relations.
pub struct AdjacencyLifecycleLinks<F> {
    read_page: F,
}

impl<F> AdjacencyLifecycleLinks<F>
where
    F: Fn(&AdjacencyRequest) -> Result<AdjacencyPage, PortError>,
{
    /// `read_page` answers one typed adjacency request, from one snapshot.
    pub fn new(read_page: F) -> Self {
        Self { read_page }
    }

    fn read(
        &self,
        node: &str,
        direction: RelationDirection,
    ) -> Result<LifecycleNeighbours, PortError> {
        let mut neighbours = LifecycleNeighbours::complete(Vec::new());
        for relation in LifecycleRelation::ALL {
            let request = AdjacencyRequest::new(node, direction, MAX_LIFECYCLE_LINKS_PER_NODE)
                .and_then(|request| request.with_relation_type(relation.as_str()))
                .map_err(|error| PortError::InvalidState(error.to_string()))?;
            let page = (self.read_page)(&request)?;
            neighbours.complete &= page.exhausted;
            neighbours
                .links
                .extend(page.edges.into_iter().map(|edge| LifecycleLink {
                    occurred_at: edge.explanation.occurred_at().map(str::to_string),
                    newer: edge.source_node_id,
                    older: edge.target_node_id,
                    relation,
                }));
        }
        Ok(neighbours)
    }
}

impl<F> LifecycleLinkSource for AdjacencyLifecycleLinks<F>
where
    F: Fn(&AdjacencyRequest) -> Result<AdjacencyPage, PortError>,
{
    fn newer_than(&self, node: &str) -> Result<LifecycleNeighbours, PortError> {
        self.read(node, RelationDirection::Incoming)
    }

    fn older_than(&self, node: &str) -> Result<LifecycleNeighbours, PortError> {
        self.read(node, RelationDirection::Outgoing)
    }
}
