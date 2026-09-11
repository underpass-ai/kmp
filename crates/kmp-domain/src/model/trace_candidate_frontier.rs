use super::{trace_path_state::TracePathState, trace_pending_states::TracePendingStates};
use crate::{
    NodeRelationProjection, TraceRoute, TraceSearchRequest, TraceSearchResult, TraceSearchStop,
};
use std::collections::{BTreeMap, BTreeSet};

/// Parent-linked path states. Adjacency admission and storage remain outside it.
pub(super) struct TraceCandidateFrontier<'a> {
    request: &'a TraceSearchRequest,
    states: Vec<TracePathState>,
    queue: TracePendingStates,
    visited: BTreeSet<String>,
    found: BTreeMap<String, Vec<usize>>,
    edges: Vec<NodeRelationProjection>,
    edge_index: BTreeMap<(String, String, String), usize>,
    pub considered: u32,
    pub stop: Option<TraceSearchStop>,
}

impl<'a> TraceCandidateFrontier<'a> {
    pub fn new(request: &'a TraceSearchRequest, root_admitted: bool, preferred: bool) -> Self {
        let mut frontier = Self {
            request,
            states: vec![],
            queue: TracePendingStates::new(request.dimensions.preferred.is_some()),
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
                preferred_nodes: u32::from(preferred),
                adjacency_offset: 0,
                expansion_started: false,
                parent: None,
                edge: None,
            });
            frontier.queue.push(0, 0, preferred);
            frontier.visited.insert(request.from.clone());
            frontier.considered = 1;
            frontier.record_target(0);
        }
        frontier
    }

    pub fn pop(&mut self) -> Option<(usize, String, u32)> {
        self.queue.pop().map(|index| {
            let state = &self.states[index];
            (index, state.node.clone(), state.depth)
        })
    }

    pub fn begin_expansion(&mut self, index: usize) -> (usize, bool) {
        let state = &mut self.states[index];
        let resumed = state.expansion_started;
        state.expansion_started = true;
        (state.adjacency_offset, resumed)
    }

    pub fn resume(&mut self, index: usize, offset: usize, preferred: bool) {
        let state = &mut self.states[index];
        state.adjacency_offset = offset;
        // A partial parent competes at child depth, after children just enqueued.
        self.queue.push(index, state.depth + 1, preferred);
    }

    /// Counts attempts before cycle/visited checks, bounding unsuccessful work too.
    pub fn extend(
        &mut self,
        parent: usize,
        neighbor: &str,
        edge: &NodeRelationProjection,
        preferred: bool,
    ) -> bool {
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
            preferred_nodes: self.states[parent].preferred_nodes + u32::from(preferred),
            adjacency_offset: 0,
            expansion_started: false,
            parent: Some(parent),
            edge: Some(edge_index),
        });
        self.queue.push(index, self.states[index].depth, preferred);
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
        if let Some(routing) = result.routing.as_mut() {
            routing.priority_pops = self.queue.priority_pops;
            routing.exploration_pops = self.queue.exploration_pops;
        }
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
                if let Some(routing) = result.routing.as_mut() {
                    routing
                        .preferred_route_entries
                        .push(self.states[last].preferred_nodes);
                }
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
