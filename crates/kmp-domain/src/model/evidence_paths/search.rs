use super::{
    EvidenceMissingWitness, EvidencePathBinding, EvidencePathBindings, EvidencePathRequest,
    EvidencePathResult, EvidencePathStatus, join, state_graph::EvidencePathStateGraph,
};
use crate::model::trace_temporal_admission::TraceTemporalAdmission;
use crate::{
    NodeRelationProjection, PortError, RelationDirection, RelationSemanticClass,
    TraceDimensionPolicy, TraceSearchStop, TraceSnapshotReader,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Discover task witnesses from a seed, sharing indexed reads and equivalent
/// product states. No destinations or answer refs are supplied to this solver.
pub fn search_evidence_paths(
    reader: &impl TraceSnapshotReader,
    request: &EvidencePathRequest,
) -> Result<EvidencePathResult, PortError> {
    request
        .validate()
        .map_err(|e| PortError::InvalidState(e.to_string()))?;
    let dimensions = TraceDimensionPolicy::default();
    let mut search = EvidencePathSearch {
        request,
        admission: TraceTemporalAdmission::for_selection(
            reader,
            &request.about,
            &request.temporal,
            &dimensions,
            request.limits,
        ),
        graph: EvidencePathStateGraph::default(),
        relations: vec![],
        relation_index: BTreeMap::new(),
        adjacency: BTreeMap::new(),
        work: 0,
        incompatible: 0,
        stop: None,
    };
    search.run()
}

struct EvidencePathSearch<'a, R> {
    request: &'a EvidencePathRequest,
    admission: TraceTemporalAdmission<'a, R>,
    graph: EvidencePathStateGraph,
    relations: Vec<NodeRelationProjection>,
    relation_index: BTreeMap<(String, String, String), u32>,
    adjacency: BTreeMap<(String, String, bool), Vec<u32>>,
    work: u32,
    incompatible: u32,
    stop: Option<TraceSearchStop>,
}

impl<R: TraceSnapshotReader> EvidencePathSearch<'_, R> {
    fn charge(&mut self) -> bool {
        if self.work == self.request.limits.states {
            self.stop = Some(TraceSearchStop::StateBudget);
            return false;
        }
        self.work += 1;
        true
    }

    fn capture(
        &mut self,
        role: usize,
        at: u32,
        node: &str,
        mut state: EvidencePathBindings,
    ) -> Result<Option<EvidencePathBindings>, PortError> {
        for binding in &self.request.roles[role].bindings {
            if binding.at() != at {
                continue;
            }
            let values = match binding {
                EvidencePathBinding::Reference { .. } => [node.to_owned()].into(),
                EvidencePathBinding::Label { key, .. } => {
                    let Some(labels) = self.admission.labels(node)? else {
                        return Ok(None);
                    };
                    let Some(values) = labels.values(key) else {
                        state.missing.insert(EvidenceMissingWitness {
                            role: self.request.roles[role].name.clone(),
                            at,
                            reference: node.into(),
                            key: key.clone(),
                            name: binding.name().into(),
                        });
                        continue;
                    };
                    values.clone()
                }
            };
            if !state.restrict(binding.name(), &values) {
                self.incompatible += 1;
                return Ok(None);
            }
        }
        Ok(Some(state))
    }

    fn edges(
        &mut self,
        node: &str,
        relation: &str,
        direction: RelationDirection,
    ) -> Result<Vec<u32>, PortError> {
        let key = (
            node.to_owned(),
            relation.to_owned(),
            direction == RelationDirection::Incoming,
        );
        if let Some(edges) = self.adjacency.get(&key) {
            return Ok(edges.clone());
        }
        let mut edges = Vec::new();
        let mut after = None;
        loop {
            let Some(page) = self
                .admission
                .budget
                .page(node, direction, after, Some(relation))?
            else {
                return Ok(edges);
            };
            for edge in page.edges {
                if edge.relation_type != relation
                    || *edge.explanation.semantic_class() == RelationSemanticClass::Structural
                    || edge
                        .explanation
                        .rationale()
                        .is_none_or(|s| s.trim().is_empty())
                    || edge
                        .explanation
                        .evidence()
                        .is_none_or(|s| s.trim().is_empty())
                    || !self
                        .admission
                        .window()
                        .admits_dependency_relation(&edge.explanation)
                {
                    continue;
                }
                let next = match direction {
                    RelationDirection::Outgoing if edge.source_node_id == node => {
                        &edge.target_node_id
                    }
                    RelationDirection::Incoming if edge.target_node_id == node => {
                        &edge.source_node_id
                    }
                    _ => {
                        return Err(PortError::InvalidState(
                            "adjacency returned an edge with the wrong endpoint".into(),
                        ));
                    }
                };
                if !self.admission.admits(next)? {
                    if self.admission.budget.stop.is_some() {
                        return Ok(edges);
                    }
                    continue;
                }
                let edge_key = (
                    edge.source_node_id.clone(),
                    edge.relation_type.clone(),
                    edge.target_node_id.clone(),
                );
                let index = if let Some(&index) = self.relation_index.get(&edge_key) {
                    if self.relations[index as usize] != edge {
                        return Err(PortError::InvalidState(
                            "inconsistent relation rows in the evidence snapshot".into(),
                        ));
                    }
                    index
                } else {
                    let index = self.relations.len() as u32;
                    self.relation_index.insert(edge_key, index);
                    self.relations.push(edge);
                    index
                };
                if !edges.contains(&index) {
                    edges.push(index);
                }
            }
            if page.exhausted {
                break;
            }
            after = page.next;
        }
        self.adjacency.insert(key, edges.clone());
        Ok(edges)
    }

    fn run(&mut self) -> Result<EvidencePathResult, PortError> {
        if !self.admission.is_owned(&self.request.from)? {
            return Err(PortError::InvalidState(
                "evidence seed must be an existing same-about entry".into(),
            ));
        }
        let resolved = self.admission.resolve_cut()?;
        let source_admitted = resolved && self.admission.admits(&self.request.from)?;
        let mut pending = VecDeque::new();
        if source_admitted {
            for role in 0..self.request.roles.len() {
                if !self.charge() {
                    break;
                }
                let root = EvidencePathBindings {
                    domains: self.request.constants.clone(),
                    ..Default::default()
                };
                if let Some(bindings) = self.capture(role, 0, &self.request.from, root)? {
                    let (index, _) = self
                        .graph
                        .add((role, 0, self.request.from.clone(), bindings), None);
                    pending.push_back(index);
                }
                if self.admission.budget.stop.is_some() {
                    break;
                }
            }
        }
        'expand: while self.stop.is_none() && self.admission.budget.stop.is_none() {
            let Some(index) = pending.pop_front() else {
                break;
            };
            let (role, position, node, bindings) = self.graph.states[index].clone();
            if position as usize == self.request.roles[role].steps.len() {
                self.graph.terminals.push(index);
                continue;
            }
            if position == self.request.limits.depth {
                self.stop = Some(TraceSearchStop::DepthBudget);
                break;
            }
            let step = self.request.roles[role].steps[position as usize].clone();
            let edges = self.edges(&node, step.relation.as_str(), step.direction)?;
            if self.admission.budget.stop.is_some() {
                break;
            }
            for edge_index in edges {
                if !self.charge() {
                    break 'expand;
                }
                let edge = &self.relations[edge_index as usize];
                let next = match step.direction {
                    RelationDirection::Outgoing => &edge.target_node_id,
                    RelationDirection::Incoming => &edge.source_node_id,
                }
                .clone();
                if let Some(next_bindings) =
                    self.capture(role, position + 1, &next, bindings.clone())?
                {
                    let (child, fresh) = self.graph.add(
                        (role, position + 1, next, next_bindings),
                        Some((index, edge_index)),
                    );
                    if fresh {
                        pending.push_back(child);
                    }
                }
                if self.admission.budget.stop.is_some() {
                    break 'expand;
                }
            }
        }
        let unknown: BTreeSet<u32> = self
            .relations
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                !self.request.temporal.is_frontier()
                    && !self.admission.window().relation_clock_known(&e.explanation)
            })
            .map(|(i, _)| i as u32)
            .collect();
        let (candidates, reconstructed) =
            self.graph
                .candidates(&unknown, &mut self.work, self.request.limits.states);
        let (groups, joined) = if reconstructed {
            join::join_candidates(
                &candidates,
                self.request.roles.len(),
                &mut self.work,
                self.request.limits.states,
            )
        } else {
            (vec![], false)
        };
        if !reconstructed || !joined {
            self.stop = Some(TraceSearchStop::StateBudget);
        }
        let mut missing_roles: Vec<_> = self
            .request
            .roles
            .iter()
            .enumerate()
            .filter(|(i, _)| !candidates.iter().any(|c| c.role == *i))
            .map(|(_, r)| r.name.clone())
            .collect();
        missing_roles.sort();
        let stop = self
            .admission
            .budget
            .stop
            .or(self.stop)
            .unwrap_or(if source_admitted {
                TraceSearchStop::FrontierExhausted
            } else {
                TraceSearchStop::SourceOutsideSelection
            });
        let status = if !matches!(
            stop,
            TraceSearchStop::FrontierExhausted | TraceSearchStop::SourceOutsideSelection
        ) {
            EvidencePathStatus::Partial
        } else {
            join::status(&groups, !missing_roles.is_empty())
        };
        Ok(EvidencePathResult {
            from: self.request.from.clone(),
            relations: std::mem::take(&mut self.relations),
            candidates,
            groups,
            missing_roles,
            status,
            stop,
            discovered_nodes: self.admission.budget.refs.len() as u32,
            scanned_edges: self.admission.budget.scanned,
            work_states: self.work,
            shared_states: self.graph.shared,
            incompatible_states: self.incompatible,
            adjacency_pages: self.admission.budget.adjacency_pages,
            coordinate_pages: self.admission.budget.coordinate_pages,
            resolved_as_of: self.admission.resolved_as_of.clone(),
            temporal_selection_resolved: resolved,
            clock_unknown_edges: unknown.into_iter().collect(),
        })
    }
}
