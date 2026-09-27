use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{BundleNode, NodeRelationProjection};
use kmp_proto_mapping::v1beta1::{LanguageSignals, LexicalProfile, LexicalReading};

use super::about_change::AboutChange;
use super::about_reader::AboutReader;
use super::kept_relation::KeptRelation;
use super::node_state::NodeState;
use super::relation_key::RelationKey;

/// How deep an ask reads an about: the default of `kmp_ask`.
pub(super) const ASK_DEPTH: u8 = 2;

/// Builds one about's part of the sidecar from nothing, reading the graph as
/// an ask of the about does: every node two hops out, every edge among them,
/// the `contains_entry` edges the selection keeps, what they bring in, and
/// the candidates and language of what is kept.
pub(super) struct AboutRebuild<'r> {
    reader: &'r AboutReader<'r>,
}

impl<'r> AboutRebuild<'r> {
    pub(super) fn new(reader: &'r AboutReader<'r>) -> Self {
        Self { reader }
    }

    pub(super) fn run(&self) -> Result<AboutChange, String> {
        let about = self.reader.about();
        // The neighbourhood, as the ask's breadth-first read reaches it.
        let mut hops = BTreeMap::from([(about.to_string(), 0u8)]);
        let mut edges: Vec<NodeRelationProjection> = Vec::new();
        let mut level = vec![about.to_string()];
        for hop in 1..=ASK_DEPTH {
            let mut next = Vec::new();
            for source in &level {
                for edge in self.reader.outgoing(source, None)? {
                    if !hops.contains_key(&edge.target_node_id) {
                        hops.insert(edge.target_node_id.clone(), hop);
                        next.push(edge.target_node_id.clone());
                    }
                    edges.push(edge);
                }
            }
            level = next;
        }
        // Edges out of the last hop count when they stay inside; the nodes
        // they reach outside are one hop past the ask's depth.
        let mut far = BTreeSet::new();
        for source in &level {
            for edge in self.reader.outgoing(source, None)? {
                if hops.contains_key(&edge.target_node_id) {
                    edges.push(edge);
                } else {
                    far.insert(edge.target_node_id);
                }
            }
        }
        let nodes = hops
            .keys()
            .map(|id| Ok((id.clone(), self.reader.node(id)?)))
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        let mut states = hops
            .iter()
            .map(|(id, hop)| {
                let evidence_kind = nodes
                    .get(id)
                    .and_then(Option::as_ref)
                    .is_some_and(|node| LexicalReading::is_evidence_kind(node.node_kind()));
                (
                    id.clone(),
                    NodeState {
                        hop: *hop,
                        evidence_kind,
                        ..NodeState::default()
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        // The selection: kept `contains_entry` edges, then the evidence that
        // supports what they keep.
        let mut kept = BTreeMap::<RelationKey, &NodeRelationProjection>::new();
        for edge in edges.iter().filter(|edge| {
            edge.relation_type == super::relation_key::CONTAINS_ENTRY
                && self.reader.keeps_contains_entry(edge)
        }) {
            kept.insert(RelationKey::of(edge), edge);
            if let Some(state) = states.get_mut(&edge.source_node_id) {
                state.selected_out += 1;
            }
            if let Some(state) = states.get_mut(&edge.target_node_id) {
                state.selected_in += 1;
            }
        }
        let selected = states
            .iter()
            .filter(|(_, state)| state.selected_entry())
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
        for edge in edges.iter().filter(|edge| {
            edge.relation_type == super::relation_key::SUPPORTS
                && selected.contains(&edge.target_node_id)
        }) {
            if let Some(state) = states.get_mut(&edge.source_node_id) {
                state.supports_selected += 1;
            }
        }
        let included = states
            .iter()
            .filter(|(_, state)| state.included())
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
        for edge in edges.iter().filter(|edge| {
            edge.relation_type != super::relation_key::CONTAINS_ENTRY
                && included.contains(&edge.source_node_id)
                && included.contains(&edge.target_node_id)
        }) {
            kept.insert(RelationKey::of(edge), edge);
        }
        // Language: what every kept node and relation says.
        let mut signals = LanguageSignals::default();
        let mut summaries = 0u64;
        let mut details = BTreeMap::new();
        for id in &included {
            let Some(Some(node)) = nodes.get(id) else {
                continue;
            };
            let detail = self.reader.detail(id)?;
            let (node_signals, summary) = AboutReader::node_unit(node, detail.as_ref());
            signals.add(&node_signals);
            summaries += u64::from(summary);
            if let Some(state) = states.get_mut(id) {
                state.signals = node_signals;
                state.carries_summary = summary;
            }
            details.insert(id.clone(), detail);
        }
        let relations = kept
            .iter()
            .map(|(key, edge)| {
                let kept = KeptRelation::of(edge);
                signals.add(&kept.signals);
                (key.clone(), Some(kept))
            })
            .collect::<Vec<_>>();
        let language = LexicalProfile::decide_language(&signals, summaries);
        let profile = LexicalProfile::new(language.clone());
        // Candidates: kept entries' text, kept evidence's details.
        let mut supported = BTreeMap::<&str, Vec<String>>::new();
        for key in kept.keys().filter(|key| key.is_supports()) {
            supported
                .entry(key.source.as_str())
                .or_default()
                .push(key.target.clone());
        }
        let mut rows = Vec::new();
        for id in &included {
            let (Some(Some(node)), Some(state)) = (nodes.get(id), states.get(id)) else {
                continue;
            };
            if state.selected_entry()
                && let Some(row) = AboutReader::entry_row(node, &profile)
            {
                rows.push((format!("entry:{id}"), Some(row.encode())));
            }
            if let Some(row) = evidence_row(node, state, &details, &supported, &profile) {
                rows.push((format!("detail:{id}"), Some(row.encode())));
            }
        }
        Ok(AboutChange {
            about: about.to_string(),
            rebuilt: true,
            nodes: states
                .into_iter()
                .map(|(id, state)| (id, Some(state)))
                .collect(),
            relations,
            rows,
            far: far.into_iter().map(|id| (id, true)).collect(),
            signals,
            summaries,
            language,
        })
    }
}

/// The row a kept evidence node's detail makes, supporting the kept nodes
/// its `supports` edges reach.
fn evidence_row(
    node: &BundleNode,
    state: &NodeState,
    details: &BTreeMap<String, Option<kmp_domain::BundleNodeDetail>>,
    supported: &BTreeMap<&str, Vec<String>>,
    profile: &LexicalProfile,
) -> Option<kmp_proto_mapping::v1beta1::LexicalRow> {
    if !state.included() || !state.evidence_kind {
        return None;
    }
    let detail = details.get(node.node_id())?.as_ref()?;
    let supports = supported.get(node.node_id()).cloned().unwrap_or_default();
    Some(AboutReader::evidence_row(node, detail, supports, profile))
}
