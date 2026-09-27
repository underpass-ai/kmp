use std::collections::{BTreeMap, BTreeSet, VecDeque};

use kmp_proto_mapping::v1beta1::{LanguageSignals, LexicalProfile, LexicalReading, LexicalRow};

use super::about_change::AboutChange;
use super::about_reader::AboutReader;
use super::about_rebuild::ASK_DEPTH;
use super::about_stats::AboutStats;
use super::node_state::NodeState;
pub(super) use super::refreshed::Refreshed;
use super::relation_key::{CONTAINS_ENTRY, RelationKey, SUPPORTS};
use super::working_set::WorkingSet;

/// Brings one built about up to date with the nodes and edges a run of
/// events touched, reading only around them (DESIGN L6, idempotent upkeep).
///
/// It follows the ask's selection the way it changes: which nodes the about
/// reaches in two hops, which `contains_entry` edges the selection keeps and
/// so which entries, labels and evidence it keeps, which relations among the
/// kept nodes stay, and then every candidate row those moves can have
/// touched. Each step recomputes from the graph as it stands, so running it
/// again over the same events changes nothing.
pub(super) struct AboutRefresh<'r, 's> {
    reader: &'r AboutReader<'r>,
    set: WorkingSet<'s>,
    about: String,
    /// Nodes whose reach changed (they came in, went out, or moved a hop),
    /// with the hop they had and the hop they have.
    hop_moves: BTreeMap<String, (Option<u8>, Option<u8>)>,
}

impl<'r, 's> AboutRefresh<'r, 's> {
    pub(super) fn new(reader: &'r AboutReader<'r>, set: WorkingSet<'s>) -> Self {
        Self {
            about: reader.about().to_string(),
            reader,
            set,
            hop_moves: BTreeMap::new(),
        }
    }

    pub(super) fn run(
        mut self,
        stats: &AboutStats,
        touched_nodes: &BTreeSet<String>,
        touched_relations: &BTreeSet<RelationKey>,
    ) -> Result<Refreshed, String> {
        // 1. Reach: who the about's asks read, and in how many hops. A node
        //    that starts or stops being one hop away moves the hop of every
        //    node it points at.
        let mut queue = touched_nodes
            .iter()
            .chain(touched_relations.iter().map(|key| &key.target))
            .cloned()
            .collect::<VecDeque<_>>();
        let mut queued = queue.iter().cloned().collect::<BTreeSet<_>>();
        while let Some(id) = queue.pop_front() {
            queued.remove(&id);
            if self.refresh_hop(&id)? {
                for edge in self.reader.outgoing(&id, None)? {
                    if queued.insert(edge.target_node_id.clone()) {
                        queue.push_back(edge.target_node_id);
                    }
                }
            }
        }
        // 1b. Nodes one hop past the ask's depth: a deeper ask reads them.
        let mut far = touched_relations
            .iter()
            .map(|key| key.target.clone())
            .chain(self.hop_moves.keys().cloned())
            .collect::<BTreeSet<_>>();
        for (id, (held, now)) in self.hop_moves.clone() {
            if held == Some(ASK_DEPTH) || now == Some(ASK_DEPTH) {
                far.extend(
                    self.reader
                        .outgoing(&id, None)?
                        .into_iter()
                        .map(|edge| edge.target_node_id),
                );
            }
        }
        for id in &far {
            self.refresh_far(id)?;
        }
        // 2. The kind of every touched node: evidence or not.
        for id in touched_nodes {
            self.refresh_kind(id)?;
        }
        // 3. The `contains_entry` edges the selection keeps.
        let mut edges = touched_relations
            .iter()
            .filter(|key| key.is_contains_entry())
            .cloned()
            .collect::<BTreeSet<_>>();
        for id in self.hop_moves.keys().cloned().collect::<Vec<_>>() {
            edges.extend(self.edges_of(&id, Some(CONTAINS_ENTRY))?);
        }
        for key in &edges {
            self.refresh_contains_entry(key)?;
        }
        for id in self.hop_moves.keys().cloned().collect::<Vec<_>>() {
            self.recount_selection(&id)?;
        }
        let selected_changed = self.changed(|state| state.selected_entry())?;
        // 4. Evidence the selection keeps for supporting a kept entry.
        let mut evidence = touched_relations
            .iter()
            .filter(|key| key.is_supports())
            .map(|key| key.source.clone())
            .chain(touched_nodes.iter().cloned())
            .chain(self.hop_moves.keys().cloned())
            .collect::<BTreeSet<_>>();
        for id in &selected_changed {
            evidence.extend(
                self.reader
                    .incoming(id, Some(SUPPORTS))?
                    .into_iter()
                    .map(|edge| edge.source_node_id),
            );
        }
        for id in &evidence {
            self.refresh_supports(id)?;
        }
        // 5. Who is kept now, and the relations the kept keep among them.
        let kept_changed = self.changed(NodeState::included)?;
        let mut relations = touched_relations
            .iter()
            .filter(|key| !key.is_contains_entry())
            .cloned()
            .collect::<BTreeSet<_>>();
        for id in &kept_changed {
            relations.extend(
                self.edges_of(id, None)?
                    .into_iter()
                    .filter(|key| !key.is_contains_entry()),
            );
        }
        for key in &relations {
            self.refresh_relation(key)?;
        }
        // 6. What each moved node says.
        for id in touched_nodes.iter().chain(&kept_changed) {
            self.refresh_unit(id)?;
        }
        // 7. The language the about reads in now; a move re-reads every row.
        let (nodes, kept_relations, removed, added, summaries) = self.set.changes();
        let mut signals = stats.signals.clone();
        signals.subtract(&removed)?;
        signals.add(&added);
        let summaries = stats
            .summaries
            .checked_sub(summaries.0)
            .ok_or("lexical index: summaries below zero")?
            + summaries.1;
        let language = LexicalProfile::decide_language(&signals, summaries);
        if language != stats.language {
            return Ok(Refreshed::LanguageMoved);
        }
        // 8. Every row the moves can have touched.
        let mut due = touched_nodes.clone();
        due.extend(kept_changed);
        due.extend(selected_changed);
        due.extend(
            relations
                .iter()
                .chain(touched_relations)
                .filter(|key| key.is_supports())
                .map(|key| key.source.clone()),
        );
        let profile = LexicalProfile::new(language.clone());
        let mut rows = Vec::with_capacity(due.len() * 2);
        for id in &due {
            rows.extend(
                self.rows_of(id, &profile)?
                    .map(|(doc, row)| (doc, row.map(|row| row.encode()))),
            );
        }
        Ok(Refreshed::Changed(AboutChange {
            about: self.about,
            rebuilt: false,
            nodes,
            relations: kept_relations,
            rows,
            far: self.set.far_changes(),
            signals,
            summaries,
            language,
        }))
    }

    /// Whether a node the about's asks do not reach lies one hop past them.
    fn refresh_far(&mut self, id: &str) -> Result<(), String> {
        let mut far = false;
        if self.set.node(id)?.is_none() {
            for edge in self.reader.incoming(id, None)? {
                if self
                    .set
                    .node(&edge.source_node_id)?
                    .is_some_and(|state| state.hop == ASK_DEPTH)
                {
                    far = true;
                    break;
                }
            }
        }
        self.set.set_far(id, far)
    }

    /// Reads a node's hop again; true when it started or stopped being one
    /// hop away, which moves the nodes it points at.
    fn refresh_hop(&mut self, id: &str) -> Result<bool, String> {
        let hop = self.hop(id)?;
        let held = self.set.node(id)?;
        let held_hop = held.as_ref().map(|state| state.hop);
        if held_hop == hop {
            return Ok(false);
        }
        self.hop_moves
            .entry(id.to_string())
            .or_insert((held_hop, hop))
            .1 = hop;
        let state = match (held, hop) {
            (_, None) => None,
            (Some(state), Some(hop)) => Some(NodeState { hop, ..state }),
            (None, Some(hop)) => Some(NodeState {
                hop,
                evidence_kind: self.is_evidence(id)?,
                ..NodeState::default()
            }),
        };
        self.set.set_node(id, state)?;
        Ok((held_hop == Some(1)) != (hop == Some(1)))
    }

    /// 0 for the about, 1 when the about points at it, 2 when something one
    /// hop away does, none past the ask's depth.
    fn hop(&mut self, id: &str) -> Result<Option<u8>, String> {
        if id == self.about {
            return Ok(Some(0));
        }
        let sources = self
            .reader
            .incoming(id, None)?
            .into_iter()
            .map(|edge| edge.source_node_id)
            .collect::<BTreeSet<_>>();
        if sources.contains(&self.about) {
            return Ok(Some(1));
        }
        for source in &sources {
            if self.set.node(source)?.is_some_and(|state| state.hop == 1) {
                return Ok(Some(2));
            }
        }
        Ok(None)
    }

    fn is_evidence(&self, id: &str) -> Result<bool, String> {
        Ok(self
            .reader
            .node(id)?
            .is_some_and(|node| LexicalReading::is_evidence_kind(node.node_kind())))
    }

    fn refresh_kind(&mut self, id: &str) -> Result<(), String> {
        if self.set.node(id)?.is_some() {
            let evidence = self.is_evidence(id)?;
            self.set
                .update_node(id, |state| state.evidence_kind = evidence)?;
        }
        Ok(())
    }

    /// Every edge into and out of a node, of one type when named.
    fn edges_of(&self, id: &str, relation_type: Option<&str>) -> Result<Vec<RelationKey>, String> {
        let mut keys = self
            .reader
            .incoming(id, relation_type)?
            .iter()
            .map(RelationKey::of)
            .collect::<Vec<_>>();
        keys.extend(
            self.reader
                .outgoing(id, relation_type)?
                .iter()
                .map(RelationKey::of),
        );
        Ok(keys)
    }

    /// Whether the selection keeps a `contains_entry` edge now, counted on
    /// both ends unless an end's reach changed (those are recounted whole).
    fn refresh_contains_entry(&mut self, key: &RelationKey) -> Result<(), String> {
        let edge = self.reader.relation(key)?;
        let kept = match &edge {
            Some(edge) => {
                self.reader.keeps_contains_entry(edge)
                    && self.set.node(&key.source)?.is_some()
                    && self.set.node(&key.target)?.is_some()
            }
            None => false,
        };
        let held = self.set.relation(key)?.is_some();
        if kept != held {
            let step = |count: &mut u64| {
                *count = if kept {
                    *count + 1
                } else {
                    count.saturating_sub(1)
                }
            };
            if !self.hop_moves.contains_key(&key.source) {
                self.set
                    .update_node(&key.source, |state| step(&mut state.selected_out))?;
            }
            if !self.hop_moves.contains_key(&key.target) {
                self.set
                    .update_node(&key.target, |state| step(&mut state.selected_in))?;
            }
        }
        let signals = edge
            .filter(|_| kept)
            .map(|edge| LanguageSignals::of_explanation(&edge.explanation));
        self.set.set_relation(key, signals)
    }

    /// Counts a node's kept `contains_entry` edges again from what is kept.
    fn recount_selection(&mut self, id: &str) -> Result<(), String> {
        if self.set.node(id)?.is_none() {
            return Ok(());
        }
        let mut selected_in = 0;
        for edge in self.reader.incoming(id, Some(CONTAINS_ENTRY))? {
            selected_in += u64::from(self.set.relation(&RelationKey::of(&edge))?.is_some());
        }
        let mut selected_out = 0;
        for edge in self.reader.outgoing(id, Some(CONTAINS_ENTRY))? {
            selected_out += u64::from(self.set.relation(&RelationKey::of(&edge))?.is_some());
        }
        self.set.update_node(id, |state| {
            state.selected_in = selected_in;
            state.selected_out = selected_out;
        })
    }

    /// The nodes of the set for which `read` came out different.
    fn changed(&mut self, read: impl Fn(&NodeState) -> bool) -> Result<BTreeSet<String>, String> {
        let mut changed = BTreeSet::new();
        for id in self.set.node_ids() {
            let before = self
                .set
                .original_node(&id)?
                .is_some_and(|state| read(&state));
            let now = self.set.node(&id)?.is_some_and(|state| read(&state));
            if before != now {
                changed.insert(id);
            }
        }
        Ok(changed)
    }

    fn refresh_supports(&mut self, id: &str) -> Result<(), String> {
        if self.set.node(id)?.is_none() {
            return Ok(());
        }
        let mut supports_selected = 0;
        for edge in self.reader.outgoing(id, Some(SUPPORTS))? {
            supports_selected += u64::from(
                self.set
                    .node(&edge.target_node_id)?
                    .is_some_and(|state| state.selected_entry()),
            );
        }
        self.set
            .update_node(id, |state| state.supports_selected = supports_selected)
    }

    /// A relation other than `contains_entry` stays when both its ends do.
    fn refresh_relation(&mut self, key: &RelationKey) -> Result<(), String> {
        let edge = self.reader.relation(key)?;
        let kept = edge.is_some()
            && self.set.node(&key.source)?.is_some_and(|s| s.included())
            && self.set.node(&key.target)?.is_some_and(|s| s.included());
        let signals = edge
            .filter(|_| kept)
            .map(|edge| LanguageSignals::of_explanation(&edge.explanation));
        self.set.set_relation(key, signals)
    }

    /// What a node adds to the language while kept; nothing otherwise.
    fn refresh_unit(&mut self, id: &str) -> Result<(), String> {
        let Some(state) = self.set.node(id)? else {
            return Ok(());
        };
        let (signals, summary) = match self.reader.node(id)?.filter(|_| state.included()) {
            Some(node) => AboutReader::node_unit(&node, self.reader.detail(id)?.as_ref()),
            None => (LanguageSignals::default(), false),
        };
        self.set.update_node(id, |state| {
            state.signals = signals;
            state.carries_summary = summary;
        })
    }

    /// The two candidates a node can make, as they stand: its text once a
    /// kept `contains_entry` reaches it, its detail while it is kept evidence.
    fn rows_of(
        &mut self,
        id: &str,
        profile: &LexicalProfile,
    ) -> Result<[(String, Option<LexicalRow>); 2], String> {
        let state = self.set.node(id)?;
        let node = match &state {
            Some(_) => self.reader.node(id)?,
            None => None,
        };
        let entry = match (&state, &node) {
            (Some(state), Some(node)) if state.selected_entry() => {
                AboutReader::entry_row(node, profile)
            }
            _ => None,
        };
        let evidence = match (&state, &node) {
            (Some(state), Some(node)) if state.included() && state.evidence_kind => {
                match self.reader.detail(id)? {
                    Some(detail) => {
                        let mut supports = Vec::new();
                        for edge in self.reader.outgoing(id, Some(SUPPORTS))? {
                            if self
                                .set
                                .node(&edge.target_node_id)?
                                .is_some_and(|s| s.included())
                            {
                                supports.push(edge.target_node_id);
                            }
                        }
                        Some(AboutReader::evidence_row(node, &detail, supports, profile))
                    }
                    None => None,
                }
            }
            _ => None,
        };
        Ok([
            (format!("entry:{id}"), entry),
            (format!("detail:{id}"), evidence),
        ])
    }
}
