use super::trace_path_state::TracePathState;
use crate::{
    NodeRelationProjection, TraceRoute, TraceSearchRequest, TraceSearchResult, TraceSearchStop,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Parent-linked BFS states. Adjacency admission and storage remain outside it.
pub(super) struct TraceCandidateFrontier<'a> {
    request: &'a TraceSearchRequest,
    states: Vec<TracePathState>,
    queue: VecDeque<usize>,
    visited: BTreeSet<String>,
    found: BTreeMap<String, Vec<usize>>,
    edges: Vec<NodeRelationProjection>,
    edge_index: BTreeMap<(String, String, String), usize>,
    pub considered: u32,
    pub stop: Option<TraceSearchStop>,
}

impl<'a> TraceCandidateFrontier<'a> {
    pub fn new(request: &'a TraceSearchRequest, root_admitted: bool) -> Self {
        let mut frontier = Self {
            request,
            states: vec![],
            queue: VecDeque::new(),
            visited: BTreeSet::new(),
            found: BTreeMap::new(),
            edges: vec![],
            edge_index: BTreeMap::new(),
            considered: 0,
            stop: None,
        };
        if root_admitted {
            frontier.states.push(TracePathState {
                node: request.from.clone(),
                depth: 0,
                parent: None,
                edge: None,
            });
            frontier.queue.push_back(0);
            frontier.visited.insert(request.from.clone());
            frontier.considered = 1;
            frontier.record_target(0);
        }
        frontier
    }

    pub fn pop(&mut self) -> Option<(usize, String, u32)> {
        self.queue.pop_front().map(|index| {
            let state = &self.states[index];
            (index, state.node.clone(), state.depth)
        })
    }

    /// Counts attempts before cycle/visited checks, bounding unsuccessful work too.
    pub fn extend(&mut self, parent: usize, neighbor: &str, edge: &NodeRelationProjection) -> bool {
        if self.considered == self.request.limits.states {
            self.stop = Some(TraceSearchStop::StateBudget);
            return false;
        }
        self.considered += 1;
        if self.request.paths_per_target == 1 {
            if !self.visited.insert(neighbor.into()) {
                return true;
            }
        } else {
            let mut ancestor = Some(parent);
            while let Some(index) = ancestor {
                if self.states[index].node == neighbor {
                    return true;
                }
                ancestor = self.states[index].parent;
            }
        }
        let key = (
            edge.source_node_id.clone(),
            edge.target_node_id.clone(),
            edge.relation_type.clone(),
        );
        let edge_index = *self.edge_index.entry(key).or_insert_with(|| {
            self.edges.push(edge.clone());
            self.edges.len() - 1
        });
        let index = self.states.len();
        self.states.push(TracePathState {
            node: neighbor.into(),
            depth: self.states[parent].depth + 1,
            parent: Some(parent),
            edge: Some(edge_index),
        });
        self.queue.push_back(index);
        self.record_target(index);
        self.stop.is_none()
    }

    fn quota(&self, target: &str) -> usize {
        if target == self.request.from {
            1
        } else {
            self.request.paths_per_target as usize
        }
    }

    fn record_target(&mut self, index: usize) {
        let node = &self.states[index].node;
        if self.request.targets.contains(node) {
            let quota = self.quota(node);
            let paths = self.found.entry(node.clone()).or_default();
            if paths.len() < quota {
                paths.push(index);
            }
            if self
                .request
                .targets
                .iter()
                .all(|t| self.found.get(t).is_some_and(|p| p.len() == self.quota(t)))
            {
                self.stop = Some(TraceSearchStop::TargetsReached);
            }
        }
    }

    pub fn finish(&self, result: &mut TraceSearchResult) {
        result.considered_states = self.considered;
        result.unreached = self
            .request
            .targets
            .iter()
            .filter(|t| !self.found.contains_key(*t))
            .cloned()
            .collect();
        result.incomplete_targets = self
            .request
            .targets
            .iter()
            .filter(|t| self.found.get(*t).map_or(0, Vec::len) < self.quota(t))
            .cloned()
            .collect();
        let mut selected = BTreeMap::new();
        for (target, states) in &self.found {
            for &last in states {
                let mut path = Vec::new();
                let mut cursor = &self.states[last];
                while let (Some(parent), Some(edge)) = (cursor.parent, cursor.edge) {
                    path.push(edge);
                    cursor = &self.states[parent];
                }
                path.reverse();
                let edge_indexes = path
                    .into_iter()
                    .map(|edge| {
                        *selected.entry(edge).or_insert_with(|| {
                            result.relations.push(self.edges[edge].clone());
                            result.relations.len() as u32 - 1
                        })
                    })
                    .collect();
                result.routes.push(TraceRoute {
                    target: target.clone(),
                    edge_indexes,
                });
            }
        }
    }
}
