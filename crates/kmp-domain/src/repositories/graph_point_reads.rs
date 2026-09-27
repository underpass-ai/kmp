use crate::{
    ContextUpdatedEvent, NodeDetailProjection, NodeProjection, NodeRelationProjection, PortError,
};

/// Point reads over one consistent snapshot of the graph and its event log.
///
/// What an index derived beside the store (the lexical sidecar, DESIGN L6)
/// needs to follow the log: where the log ends, the events after a position,
/// and the few nodes, bodies and edges those events touched. Every method of
/// one reader answers from the same snapshot, so a position read here and the
/// graph read here agree. Implementations must not open a transaction per
/// call.
pub trait GraphPointReads {
    /// The sequence of the last event in the log; 0 when it is empty.
    fn last_event_sequence(&self) -> Result<u64, PortError>;

    /// The event stored at `sequence`, if any.
    fn event(&self, sequence: u64) -> Result<Option<ContextUpdatedEvent>, PortError>;

    fn node(&self, node_id: &str) -> Result<Option<NodeProjection>, PortError>;

    fn detail(&self, node_id: &str) -> Result<Option<NodeDetailProjection>, PortError>;

    /// One edge by its identity, if it exists.
    fn relation(
        &self,
        source: &str,
        target: &str,
        relation_type: &str,
    ) -> Result<Option<NodeRelationProjection>, PortError>;

    /// Every edge out of a node, of one type when named, ascending by target.
    fn outgoing(
        &self,
        node_id: &str,
        relation_type: Option<&str>,
    ) -> Result<Vec<NodeRelationProjection>, PortError>;

    /// How many edges of one type leave a node, without reading them.
    fn outgoing_count(&self, node_id: &str, relation_type: &str) -> Result<u64, PortError> {
        Ok(self.outgoing(node_id, Some(relation_type))?.len() as u64)
    }

    /// Every edge into a node, of one type when named, ascending by source.
    fn incoming(
        &self,
        node_id: &str,
        relation_type: Option<&str>,
    ) -> Result<Vec<NodeRelationProjection>, PortError>;
}
