use crate::{
    AdjacencyPage, AdjacencyRequest, NodeProjection, PortError, RelationDirection,
    RelationPosition, TraceSearchLimits, TraceSearchStop, TraceSnapshotReader,
};
use std::collections::BTreeSet;

pub(super) struct TraceReadBudget<'a, R> {
    reader: &'a R,
    limits: TraceSearchLimits,
    pub refs: BTreeSet<String>,
    pub scanned: u32,
    pub coordinate_rows: u32,
    pub adjacency_pages: u32,
    pub coordinate_pages: u32,
    pub stop: Option<TraceSearchStop>,
    pub proof_reads: bool,
}

impl<'a, R: TraceSnapshotReader> TraceReadBudget<'a, R> {
    pub fn new(reader: &'a R, limits: TraceSearchLimits) -> Self {
        Self {
            reader,
            limits,
            refs: BTreeSet::new(),
            scanned: 0,
            coordinate_rows: 0,
            adjacency_pages: 0,
            coordinate_pages: 0,
            stop: None,
            proof_reads: false,
        }
    }

    pub fn node(&mut self, id: &str) -> Result<Option<NodeProjection>, PortError> {
        if !self.refs.contains(id) && self.refs.len() == self.limits.nodes as usize {
            self.stop = Some(TraceSearchStop::NodeBudget);
            return Ok(None);
        }
        self.refs.insert(id.into());
        self.reader.node(id)
    }

    pub fn page(
        &mut self,
        node: &str,
        direction: RelationDirection,
        after: Option<RelationPosition>,
        relation_type: Option<&str>,
    ) -> Result<Option<AdjacencyPage>, PortError> {
        self.page_limited(node, direction, after, relation_type, 32)
    }

    pub fn page_limited(
        &mut self,
        node: &str,
        direction: RelationDirection,
        after: Option<RelationPosition>,
        relation_type: Option<&str>,
        rows: u32,
    ) -> Result<Option<AdjacencyPage>, PortError> {
        if self.proof_reads {
            return self.proof_page(node, direction, after, relation_type, rows);
        }
        let nodes_left = self.limits.nodes - self.refs.len() as u32;
        let edges_left = self.limits.edges - self.scanned;
        if nodes_left == 0 || edges_left == 0 {
            self.stop = Some(if nodes_left == 0 {
                TraceSearchStop::NodeBudget
            } else {
                TraceSearchStop::EdgeBudget
            });
            return Ok(None);
        }
        let mut request =
            AdjacencyRequest::new(node, direction, nodes_left.min(edges_left).min(rows))
                .map_err(|e| PortError::InvalidState(e.to_string()))?;
        if let Some(kind) = relation_type {
            request = request
                .with_relation_type(kind)
                .map_err(|e| PortError::InvalidState(e.to_string()))?;
        }
        if let Some(after) = after {
            request = request.with_after(after);
        }
        let page = self.reader.adjacency(&request)?;
        self.scanned += page.edges.len() as u32;
        if relation_type == Some("contains_entry") {
            self.coordinate_rows += page.edges.len() as u32;
            self.coordinate_pages += 1;
        } else {
            self.adjacency_pages += 1;
        }
        for edge in &page.edges {
            self.refs.insert(
                match direction {
                    RelationDirection::Outgoing => &edge.target_node_id,
                    RelationDirection::Incoming => &edge.source_node_id,
                }
                .clone(),
            );
        }
        Ok(Some(page))
    }
    /// Proof may reuse already charged endpoints at N exactly. Fetch at most
    /// one new endpoint beyond remaining capacity, solely to detect a cutoff;
    /// that row is scanned under E but is never admitted or reported missing.
    fn proof_page(
        &mut self,
        node: &str,
        direction: RelationDirection,
        after: Option<RelationPosition>,
        relation_type: Option<&str>,
        rows: u32,
    ) -> Result<Option<AdjacencyPage>, PortError> {
        if self.stop.is_some() {
            return Ok(None);
        }
        let edges_left = self.limits.edges - self.scanned;
        if edges_left == 0 {
            self.stop = Some(TraceSearchStop::EdgeBudget);
            return Ok(None);
        }
        let nodes_left = self.limits.nodes - self.refs.len() as u32;
        let mut request =
            AdjacencyRequest::new(node, direction, nodes_left.max(1).min(edges_left).min(rows))
                .map_err(|e| PortError::InvalidState(e.to_string()))?;
        if let Some(kind) = relation_type {
            request = request
                .with_relation_type(kind)
                .map_err(|e| PortError::InvalidState(e.to_string()))?;
        }
        if let Some(after) = after {
            request = request.with_after(after);
        }
        let page = self.reader.adjacency(&request)?;
        self.scanned += page.edges.len() as u32;
        if relation_type == Some("contains_entry") {
            self.coordinate_rows += page.edges.len() as u32;
            self.coordinate_pages += 1;
        } else {
            self.adjacency_pages += 1;
        }
        for edge in &page.edges {
            let id = match direction {
                RelationDirection::Outgoing => &edge.target_node_id,
                RelationDirection::Incoming => &edge.source_node_id,
            };
            if !self.refs.contains(id) && self.refs.len() == self.limits.nodes as usize {
                self.stop = Some(TraceSearchStop::NodeBudget);
                return Ok(None);
            }
            self.refs.insert(id.clone());
        }
        Ok(Some(page))
    }
}
