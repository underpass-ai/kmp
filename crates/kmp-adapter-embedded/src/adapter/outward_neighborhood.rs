use std::collections::{BTreeMap, BTreeSet, VecDeque};

use kmp_domain::{NeighborhoodRequest, NodeRelationProjection, PortError};

use super::{
    engine::{ReadTx, Table},
    serdes::decode_explanation,
};

type RelationKey = (String, String, String);
type RawRelations = BTreeMap<RelationKey, Vec<u8>>;
type ReadResult = (BTreeSet<String>, RawRelations);

/// One outward traversal and its retained relations over one read transaction.
///
/// Raw explanations live only for edges that can appear in the result. This
/// keeps reuse bounded by the response payload while allowing every selected
/// source adjacency to be scanned once.
pub(super) struct OutwardNeighborhoodRead<'a> {
    tx: &'a dyn ReadTx,
    request: &'a NeighborhoodRequest,
}

impl<'a> OutwardNeighborhoodRead<'a> {
    pub(super) fn new(tx: &'a dyn ReadTx, request: &'a NeighborhoodRequest) -> Self {
        Self { tx, request }
    }

    pub(super) fn catalogue(
        self,
    ) -> Result<(BTreeSet<String>, Vec<NodeRelationProjection>), PortError> {
        if self.request.depth() == 0 {
            return Ok((BTreeSet::new(), Vec::new()));
        }
        let root = self.request.root_node_id().to_string();
        let (selected, retained) = self.read(BTreeSet::from([root.clone()]))?;
        let mut reachable = selected;
        reachable.remove(&root);
        if reachable.is_empty() {
            // Preserve the existing depth-zero and isolated-root contract:
            // catalogue relations are empty when there are no neighbors.
            return Ok((reachable, Vec::new()));
        }
        Ok((reachable, Self::decode(retained)?))
    }

    pub(super) fn extending(
        self,
        mut selected: BTreeSet<String>,
    ) -> Result<(BTreeSet<String>, Vec<NodeRelationProjection>), PortError> {
        selected.insert(self.request.root_node_id().to_string());
        let (selected, retained) = self.read(selected)?;
        Ok((selected, Self::decode(retained)?))
    }

    fn read(self, mut selected: BTreeSet<String>) -> Result<ReadResult, PortError> {
        let root = self.request.root_node_id();
        let mut visited = BTreeSet::from([root.to_string()]);
        let mut frontier = VecDeque::from([(root.to_string(), 0u32)]);
        let mut scanned = BTreeSet::new();
        let mut retained = RawRelations::new();

        while let Some((source, hops)) = frontier.pop_front() {
            if hops == self.request.depth() {
                continue;
            }
            scanned.insert(source.clone());
            for (key, raw) in self.tx.scan_str3_by_first(Table::Relations, &source)? {
                let target = key.1.clone();
                let admitted = self.request.admits(&target);
                if selected.contains(&target) || admitted {
                    retained.insert(key, raw);
                }
                if admitted && visited.insert(target.clone()) {
                    selected.insert(target.clone());
                    frontier.push_back((target.clone(), hops + 1));
                }
            }
        }

        // Sources at the depth boundary and any caller-selected path ancestors
        // were not traversed. Read each once now that final membership is known.
        for source in &selected {
            if scanned.contains(source) {
                continue;
            }
            for (key, raw) in self.tx.scan_str3_by_first(Table::Relations, source)? {
                if selected.contains(&key.1) {
                    retained.insert(key, raw);
                }
            }
        }

        Ok((selected, retained))
    }

    fn decode(retained: RawRelations) -> Result<Vec<NodeRelationProjection>, PortError> {
        retained
            .into_iter()
            .map(|((source_node_id, target_node_id, relation_type), raw)| {
                Ok(NodeRelationProjection {
                    source_node_id,
                    target_node_id,
                    relation_type,
                    explanation: decode_explanation(&raw)?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "outward_neighborhood_tests.rs"]
mod tests;
