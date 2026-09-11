use super::{EvidencePathCandidate, state::EvidencePathState};
use std::collections::{BTreeMap, BTreeSet};

/// Acyclic product of finite role position and stored node. Multiple parents
/// retain distinct proofs even when equivalent states share one expansion.
#[derive(Default)]
pub(super) struct EvidencePathStateGraph {
    pub states: Vec<EvidencePathState>,
    index: BTreeMap<EvidencePathState, usize>,
    parents: Vec<BTreeSet<(usize, u32)>>,
    pub terminals: Vec<usize>,
    pub shared: u32,
}

impl EvidencePathStateGraph {
    pub fn add(&mut self, state: EvidencePathState, parent: Option<(usize, u32)>) -> (usize, bool) {
        if let Some(&index) = self.index.get(&state) {
            if let Some(parent) = parent {
                self.parents[index].insert(parent);
            }
            self.shared += 1;
            return (index, false);
        }
        let index = self.states.len();
        self.index.insert(state.clone(), index);
        self.states.push(state);
        self.parents.push(parent.into_iter().collect());
        (index, true)
    }

    /// Enumerate proof alternatives only after expansion has shared states and
    /// indexed adjacency. The work budget also bounds this reconstruction.
    pub fn candidates(
        &self,
        unknown_edges: &BTreeSet<u32>,
        work: &mut u32,
        max_work: u32,
    ) -> (Vec<EvidencePathCandidate>, bool) {
        let mut found = Vec::new();
        for &terminal in &self.terminals {
            // One DFS stack and one mutable path: do not clone a long suffix
            // for every parent before the reconstruction budget admits it.
            let mut pending = vec![(terminal, self.parents[terminal].iter(), false)];
            let mut nodes = vec![self.states[terminal].node.clone()];
            let mut edges: Vec<u32> = Vec::new();
            while let Some((index, parents, entered)) = pending.last_mut() {
                if !*entered {
                    if *work == max_work {
                        return (found, false);
                    }
                    *work += 1;
                    *entered = true;
                }
                let state = &self.states[*index];
                if state.position == 0 && state.context_hops == 0 {
                    let route_nodes = nodes.iter().rev().cloned().collect();
                    let edge_indexes: Vec<_> = edges.iter().rev().copied().collect();
                    let clock_unknown = edge_indexes.iter().any(|i| unknown_edges.contains(i));
                    found.push(EvidencePathCandidate {
                        role: state.role,
                        context_hops: self.states[terminal].context_hops,
                        nodes: route_nodes,
                        edge_indexes,
                        bindings: self.states[terminal].bindings.clone(),
                        clock_unknown,
                    });
                } else if let Some(&(parent, edge)) = parents.next() {
                    nodes.push(self.states[parent].node.clone());
                    edges.push(edge);
                    pending.push((parent, self.parents[parent].iter(), false));
                    continue;
                }
                pending.pop();
                nodes.pop();
                edges.pop();
            }
        }
        (found, true)
    }
}
