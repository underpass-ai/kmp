use std::collections::{BTreeMap, BTreeSet};

use crate::{
    DomainError, KmpBundle, ProofDependencyGroup, TemporalCoordinate, TemporalCursor,
    TemporalReadWindow, TemporalTraversalResult,
};

/// Selects the proof identities of a temporal page without inspecting bodies.
/// The graph must already carry the read's dimension and membership admission.
#[derive(Debug)]
pub struct TemporalProofPlan {
    groups: Vec<ProofDependencyGroup>,
    body_refs: BTreeSet<String>,
}

impl TemporalProofPlan {
    pub fn select(
        bundle: &KmpBundle,
        traversal: &TemporalTraversalResult,
        dependencies: bool,
    ) -> Result<Self, DomainError> {
        let selection = traversal.proof_selection();
        let resolved = match selection.cursor() {
            Some(TemporalCursor::Time(at)) => Some(at.as_str()),
            _ => None,
        };
        let window = TemporalReadWindow::new(&selection, resolved);
        let nodes = std::iter::once(bundle.root_node())
            .chain(bundle.neighbor_nodes())
            .map(|node| (node.node_id(), node))
            .collect::<BTreeMap<_, _>>();
        let groups = if dependencies {
            let mut scoped = BTreeSet::new();
            let mut eligible = BTreeSet::new();
            for edge in bundle
                .relationships()
                .iter()
                .filter(|edge| edge.relationship_type() == "contains_entry")
            {
                let id = edge.target_node_id();
                scoped.insert(id.to_string());
                if nodes
                    .get(id)
                    .is_none_or(|node| node.summary().trim().is_empty())
                {
                    continue;
                }
                if selection.is_frontier()
                    || TemporalCoordinate::from_relation_explanation(edge.explanation())?
                        .is_some_and(|coordinate| window.admits_coordinate(&coordinate))
                {
                    eligible.insert(id.to_string());
                }
            }
            let edges = bundle
                .relationships()
                .iter()
                .filter(|edge| window.admits_dependency_relation(edge.explanation()))
                .cloned()
                .collect::<Vec<_>>();
            traversal
                .entries()
                .iter()
                .filter(|entry| eligible.contains(entry.ref_id()))
                .map(|entry| {
                    ProofDependencyGroup::select(entry.ref_id(), &scoped, &eligible, &edges)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut body_refs = traversal
            .entries()
            .iter()
            .map(|entry| entry.ref_id().to_string())
            .chain(
                groups
                    .iter()
                    .flat_map(|group| group.member_refs().iter().cloned()),
            )
            .collect::<BTreeSet<_>>();
        let proof_refs = body_refs.clone();
        // Receipt-time and relation admission still run in response projection.
        // Asking for a source here cannot make it visible in the response.
        for edge in bundle.relationships().iter().filter(|edge| {
            edge.relationship_type() == "supports" && proof_refs.contains(edge.target_node_id())
        }) {
            if nodes
                .get(edge.source_node_id())
                .is_some_and(|node| matches!(node.node_kind(), "memory_evidence" | "evidence"))
            {
                body_refs.insert(edge.source_node_id().to_string());
            }
        }
        Ok(Self { groups, body_refs })
    }

    pub fn groups(&self) -> &[ProofDependencyGroup] {
        &self.groups
    }
    pub fn body_refs(&self) -> &BTreeSet<String> {
        &self.body_refs
    }
}
