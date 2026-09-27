use std::collections::BTreeMap;

use kmp_proto_mapping::v1beta1::LanguageSignals;

use super::node_state::NodeState;
use super::relation_key::RelationKey;
use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;

/// The node states and kept relations of one about while a refresh works on
/// them: read from the sidecar the first time they are asked for, changed in
/// memory, and reported as a change only where they end up different from
/// what the sidecar held.
pub(super) struct WorkingSet<'s> {
    sidecar: &'s SqliteLexicalSidecar,
    about: &'s str,
    nodes: BTreeMap<String, (Option<NodeState>, Option<NodeState>)>,
    relations: BTreeMap<RelationKey, (Option<LanguageSignals>, Option<LanguageSignals>)>,
    /// Nodes one hop past the ask's depth, as held and now.
    far: BTreeMap<String, (bool, bool)>,
}

impl<'s> WorkingSet<'s> {
    pub(super) fn new(sidecar: &'s SqliteLexicalSidecar, about: &'s str) -> Self {
        Self {
            sidecar,
            about,
            nodes: BTreeMap::new(),
            relations: BTreeMap::new(),
            far: BTreeMap::new(),
        }
    }

    /// Sets whether a node lies one hop past the ask's depth.
    pub(super) fn set_far(&mut self, id: &str, far: bool) -> Result<(), String> {
        if !self.far.contains_key(id) {
            let held = self.sidecar.is_far(self.about, id)?;
            self.far.insert(id.to_string(), (held, held));
        }
        if let Some(entry) = self.far.get_mut(id) {
            entry.1 = far;
        }
        Ok(())
    }

    /// The nodes whose place past the ask's depth changed, and where they are.
    pub(super) fn far_changes(&self) -> Vec<(String, bool)> {
        self.far
            .iter()
            .filter(|(_, (held, now))| held != now)
            .map(|(id, (_, now))| (id.clone(), *now))
            .collect()
    }

    fn load_node(&mut self, id: &str) -> Result<(), String> {
        if !self.nodes.contains_key(id) {
            let held = self.sidecar.node(self.about, id)?;
            self.nodes.insert(id.to_string(), (held.clone(), held));
        }
        Ok(())
    }

    /// The node's state now; `None` when the about's asks do not reach it.
    pub(super) fn node(&mut self, id: &str) -> Result<Option<NodeState>, String> {
        self.load_node(id)?;
        Ok(self.nodes[id].1.clone())
    }

    /// The node's state as the sidecar held it before this refresh.
    pub(super) fn original_node(&mut self, id: &str) -> Result<Option<NodeState>, String> {
        self.load_node(id)?;
        Ok(self.nodes[id].0.clone())
    }

    pub(super) fn set_node(&mut self, id: &str, state: Option<NodeState>) -> Result<(), String> {
        self.load_node(id)?;
        if let Some(entry) = self.nodes.get_mut(id) {
            entry.1 = state;
        }
        Ok(())
    }

    /// Changes a node's state in place; nothing when the node is not reached.
    pub(super) fn update_node(
        &mut self,
        id: &str,
        change: impl FnOnce(&mut NodeState),
    ) -> Result<(), String> {
        self.load_node(id)?;
        if let Some((_, Some(state))) = self.nodes.get_mut(id) {
            change(state);
        }
        Ok(())
    }

    fn load_relation(&mut self, key: &RelationKey) -> Result<(), String> {
        if !self.relations.contains_key(key) {
            let held = self.sidecar.relation(self.about, key)?;
            self.relations.insert(key.clone(), (held.clone(), held));
        }
        Ok(())
    }

    /// The signals of a relation the selection keeps; `None` when it does not.
    pub(super) fn relation(
        &mut self,
        key: &RelationKey,
    ) -> Result<Option<LanguageSignals>, String> {
        self.load_relation(key)?;
        Ok(self.relations[key].1.clone())
    }

    pub(super) fn set_relation(
        &mut self,
        key: &RelationKey,
        signals: Option<LanguageSignals>,
    ) -> Result<(), String> {
        self.load_relation(key)?;
        if let Some(entry) = self.relations.get_mut(key) {
            entry.1 = signals;
        }
        Ok(())
    }

    /// The ids of every node this refresh looked at.
    pub(super) fn node_ids(&self) -> Vec<String> {
        self.nodes.keys().cloned().collect()
    }

    /// What moved, as the change lists of an `AboutChange`, with the language
    /// the moved nodes and relations took away from the about (`removed`) and
    /// brought to it (`added`), and the summaries they took and brought.
    #[allow(clippy::type_complexity)]
    pub(super) fn changes(
        &self,
    ) -> (
        Vec<(String, Option<NodeState>)>,
        Vec<(RelationKey, Option<LanguageSignals>)>,
        LanguageSignals,
        LanguageSignals,
        (u64, u64),
    ) {
        let mut removed = LanguageSignals::default();
        let mut added = LanguageSignals::default();
        let mut summaries = (0u64, 0u64);
        let mut nodes = Vec::new();
        for (id, (held, now)) in &self.nodes {
            if held == now {
                continue;
            }
            if let Some(held) = held.as_ref().filter(|state| state.included()) {
                removed.add(&held.signals);
                summaries.0 += u64::from(held.carries_summary);
            }
            if let Some(now) = now.as_ref().filter(|state| state.included()) {
                added.add(&now.signals);
                summaries.1 += u64::from(now.carries_summary);
            }
            nodes.push((id.clone(), now.clone()));
        }
        let mut relations = Vec::new();
        for (key, (held, now)) in &self.relations {
            if held == now {
                continue;
            }
            if let Some(held) = held {
                removed.add(held);
            }
            if let Some(now) = now {
                added.add(now);
            }
            relations.push((key.clone(), now.clone()));
        }
        (nodes, relations, removed, added, summaries)
    }
}
