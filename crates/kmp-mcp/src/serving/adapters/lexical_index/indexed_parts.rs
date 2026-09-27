use std::collections::{BTreeMap, BTreeSet};

use kmp_application::RecallParts;
use kmp_domain::{GraphPointReads, NodeNeighborhood, NodeProjection, NodeRelationProjection};

use super::node_state::NodeState;
use super::relation_key::{CONTAINS_ENTRY, RelationKey, SUPPORTS};
use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;

/// How many hops from the candidates the ranker's rescues walk: the reach
/// graph two, a restatement one. Lifecycle links are followed to their ends.
const WALKED_HOPS: usize = 2;

/// The relation types that declare a lifecycle, whose chains a rescue walks
/// whole.
const LIFECYCLE: [&str; 3] = ["supersedes", "corrects", "updates_state"];

/// The part of an about an ask answered from the postings reads (DESIGN L6,
/// P13), from one snapshot of the store and the sidecar that followed it to
/// the same position:
///
/// - the candidates the postings reached, and the entries their evidence
///   supports;
/// - every node two kept hops from those, lifecycle chains followed whole:
///   what a restatement, the reach graph or a lifecycle rescue can bring in;
/// - for every such node, the labels that keep it, the anchor's edge to it
///   and, for an entry, the evidence that supports it, and for evidence,
///   what it supports;
/// - every edge the selection keeps among all of these.
///
/// Every candidate that can score is here, and every one the ranker's
/// rescues can reach from those that do; what else is here scores nothing
/// and reaches nothing the ranker could return.
pub(super) struct IndexedParts<'a> {
    reads: &'a dyn GraphPointReads,
    sidecar: &'a SqliteLexicalSidecar,
    about: &'a str,
    states: BTreeMap<String, Option<NodeState>>,
    nodes: BTreeMap<String, NodeProjection>,
    relations: BTreeMap<RelationKey, NodeRelationProjection>,
}

fn port(error: kmp_domain::PortError) -> String {
    format!("lexical index: {error}")
}

impl<'a> IndexedParts<'a> {
    pub(super) fn new(
        reads: &'a dyn GraphPointReads,
        sidecar: &'a SqliteLexicalSidecar,
        about: &'a str,
    ) -> Self {
        Self {
            reads,
            sidecar,
            about,
            states: BTreeMap::new(),
            nodes: BTreeMap::new(),
            relations: BTreeMap::new(),
        }
    }

    /// The parts for `candidates` (`entry:<node>` and `detail:<node>`
    /// documents), or `None` when the about has no root.
    pub(super) fn read(
        mut self,
        candidates: &BTreeSet<String>,
    ) -> Result<Option<RecallParts>, String> {
        let Some(root) = self.reads.node(self.about).map_err(port)? else {
            return Ok(None);
        };
        let core = candidates
            .iter()
            .filter_map(|doc| {
                doc.strip_prefix("entry:")
                    .or_else(|| doc.strip_prefix("detail:"))
            })
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        // The entries the candidate evidence supports are what the ranker
        // walks from.
        let mut level = BTreeSet::new();
        for id in &core {
            level.insert(id.clone());
            for edge in self.kept_edges(id, true, false, Some(SUPPORTS))? {
                level.insert(edge.target_node_id.clone());
            }
        }
        let mut walked = BTreeSet::new();
        for hop in 0..=WALKED_HOPS {
            let mut next = BTreeSet::new();
            let mut pending = level.into_iter().collect::<Vec<_>>();
            while let Some(id) = pending.pop() {
                if !walked.insert(id.clone()) || !self.walkable(&id)? {
                    continue;
                }
                self.add_node(&id)?;
                for edge in self.kept_edges(&id, true, true, None)? {
                    let other = if edge.source_node_id == id {
                        edge.target_node_id.clone()
                    } else {
                        edge.source_node_id.clone()
                    };
                    let lifecycle = LIFECYCLE.contains(&edge.relation_type.as_str());
                    self.add_edge(edge)?;
                    if walked.contains(&other) {
                        continue;
                    }
                    if lifecycle {
                        // A chain is walked whole, at the hop it started from.
                        pending.push(other);
                    } else if hop < WALKED_HOPS {
                        next.insert(other);
                    }
                }
            }
            level = next;
        }
        // What makes every node read a candidate as the whole about makes
        // it one: its labels and the anchor's edge; for an entry walked, the
        // evidence that supports it; for evidence, everything it supports.
        let mut completed = BTreeSet::new();
        let mut pending = self.nodes.keys().cloned().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if id == self.about || !completed.insert(id.clone()) {
                continue;
            }
            let before = self.nodes.len();
            let mut edges = self.kept_edges(&id, false, true, Some(CONTAINS_ENTRY))?;
            edges.extend(
                self.kept_edges(&id, false, true, None)?
                    .into_iter()
                    .filter(|edge| edge.source_node_id == self.about),
            );
            if walked.contains(&id) {
                edges.extend(self.kept_edges(&id, false, true, Some(SUPPORTS))?);
            }
            if self.state(&id)?.is_some_and(|state| state.evidence_kind) {
                edges.extend(self.kept_edges(&id, true, false, Some(SUPPORTS))?);
            }
            for edge in edges {
                self.add_edge(edge)?;
            }
            if self.nodes.len() != before {
                pending.extend(
                    self.nodes
                        .keys()
                        .filter(|known| !completed.contains(*known))
                        .cloned(),
                );
            }
        }
        // Every kept edge among what was read.
        for id in self.nodes.keys().cloned().collect::<Vec<_>>() {
            if self.is_label(&id)? {
                continue;
            }
            for edge in self.kept_edges(&id, true, false, None)? {
                if self.nodes.contains_key(&edge.target_node_id) {
                    self.add_edge(edge)?;
                }
            }
        }
        let mut details = BTreeMap::new();
        for id in self.nodes.keys() {
            if let Some(detail) = self.reads.detail(id).map_err(port)? {
                details.insert(id.clone(), detail);
            }
        }
        let neighbors = self
            .nodes
            .into_iter()
            .filter(|(id, _)| id.as_str() != self.about)
            .map(|(_, node)| node)
            .collect();
        Ok(Some(RecallParts {
            neighborhood: NodeNeighborhood {
                root,
                neighbors,
                relations: self.relations.into_values().collect(),
            },
            details,
            read_revision: None,
        }))
    }

    fn state(&mut self, id: &str) -> Result<Option<NodeState>, String> {
        if !self.states.contains_key(id) {
            let state = self.sidecar.node(self.about, id)?;
            self.states.insert(id.to_string(), state);
        }
        Ok(self.states[id].clone())
    }

    fn included(&mut self, id: &str) -> Result<bool, String> {
        Ok(self.state(id)?.is_some_and(|state| state.included()))
    }

    /// A label keeps entries; it is never walked through.
    fn is_label(&mut self, id: &str) -> Result<bool, String> {
        Ok(self
            .state(id)?
            .is_some_and(|state| state.selected_out > 0 && state.selected_in == 0))
    }

    /// Whether the walk goes on from a node: a kept node of the about that
    /// is neither the anchor nor a label.
    fn walkable(&mut self, id: &str) -> Result<bool, String> {
        Ok(id != self.about && self.included(id)? && !self.is_label(id)?)
    }

    fn add_node(&mut self, id: &str) -> Result<(), String> {
        if !self.nodes.contains_key(id)
            && let Some(node) = self.reads.node(id).map_err(port)?
        {
            self.nodes.insert(id.to_string(), node);
        }
        Ok(())
    }

    fn add_edge(&mut self, edge: NodeRelationProjection) -> Result<(), String> {
        self.add_node(&edge.source_node_id.clone())?;
        self.add_node(&edge.target_node_id.clone())?;
        self.relations.insert(RelationKey::of(&edge), edge);
        Ok(())
    }

    /// The edges the selection keeps at a node, out of it and into it.
    fn kept_edges(
        &mut self,
        id: &str,
        outgoing: bool,
        incoming: bool,
        relation_type: Option<&str>,
    ) -> Result<Vec<NodeRelationProjection>, String> {
        let mut edges = Vec::new();
        if outgoing {
            edges.extend(self.reads.outgoing(id, relation_type).map_err(port)?);
        }
        if incoming {
            edges.extend(self.reads.incoming(id, relation_type).map_err(port)?);
        }
        let mut kept = Vec::with_capacity(edges.len());
        for edge in edges {
            if self
                .sidecar
                .relation(self.about, &RelationKey::of(&edge))?
                .is_some()
            {
                kept.push(edge);
            }
        }
        Ok(kept)
    }
}
