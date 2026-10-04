//! The source half of `import --from --about`: a verified bundle narrowed to
//! the requested abouts, derived, and the checks only it can answer.

use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{ContextUpdatedEvent, PortError, ProjectionMutation};

use super::engine::{Key, Table, WriteTx};
use super::portability::{parse_bundle, validate_revisions};
use super::serdes::{NodeRecord, decode};

/// The node kind the projection writes for a relation endpoint nothing has
/// materialised: the visible form of a broken relation.
const PLACEHOLDER_KIND: &str = "placeholder";

/// The source side, verified and derived before any destination lock is
/// taken: the requested abouts' events in source order and the projection
/// each one produces.
pub(super) struct SourceAbouts {
    events: Vec<ContextUpdatedEvent>,
    mutations: Vec<Vec<ProjectionMutation>>,
    /// Every node the requested abouts materialise in the source.
    materialised: BTreeSet<String>,
}

impl SourceAbouts {
    pub(super) fn read<F>(bundle: &str, requested: &[String], derive: F) -> Result<Self, PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError>,
    {
        if requested.is_empty() {
            return Err(PortError::InvalidState(
                "import --from needs at least one --about".to_string(),
            ));
        }
        let verified = parse_bundle(bundle)?;
        let held = verified.header.abouts.iter().collect::<BTreeSet<_>>();
        let missing = requested
            .iter()
            .filter(|about| !held.contains(about))
            .collect::<BTreeSet<_>>();
        if !missing.is_empty() {
            return Err(PortError::InvalidState(format!(
                "the source holds no about {}; nothing was written",
                missing
                    .iter()
                    .map(|about| format!("`{about}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        let wanted = requested.iter().collect::<BTreeSet<_>>();
        let events = verified
            .events
            .into_iter()
            .filter(|event| wanted.contains(&event.root_node_id))
            .collect::<Vec<_>>();
        validate_revisions(&events)?;
        // Derivation is pure: prove every payload projects before the
        // destination is locked, so a malformed event cannot stop halfway.
        let mutations = events.iter().map(&derive).collect::<Result<Vec<_>, _>>()?;
        let materialised = mutations
            .iter()
            .flatten()
            .filter_map(|mutation| match mutation {
                ProjectionMutation::EnsureNode(node) | ProjectionMutation::UpsertNode(node) => {
                    Some(node.node_id.clone())
                }
                _ => None,
            })
            .collect();
        Ok(Self {
            events,
            mutations,
            materialised,
        })
    }

    /// The requested abouts' events, in source order.
    pub(super) fn events(&self) -> &[ContextUpdatedEvent] {
        &self.events
    }

    /// The projection the event at `position` produces.
    pub(super) fn mutations(&self, position: usize) -> &[ProjectionMutation] {
        &self.mutations[position]
    }

    /// An idempotency key already recorded here answers for some other write;
    /// appending would overwrite that answer.
    pub(super) fn refuse_idempotency_collisions(
        &self,
        tx: &dyn WriteTx,
        appended: &[usize],
    ) -> Result<(), PortError> {
        let mut collisions = Vec::new();
        for position in appended {
            let event = &self.events[*position];
            if let Some(key) = event.idempotency_key.as_deref()
                && tx.get(Table::Idempotency, Key::Str(key))?.is_some()
            {
                collisions.push(format!(
                    "`{}` revision {} (idempotency key `{key}`)",
                    event.root_node_id, event.revision
                ));
            }
        }
        if collisions.is_empty() {
            return Ok(());
        }
        Err(PortError::Conflict(format!(
            "import refused; nothing was written. These events reuse an idempotency key this \
             store already recorded for another write: {}",
            collisions.join(", ")
        )))
    }

    /// A relation whose endpoint is materialised neither by the imported
    /// abouts nor already here would become an edge to a placeholder: a
    /// broken relation. Name each one and the endpoint it is missing, so the
    /// operator can add the about that owns it.
    pub(super) fn refuse_dangling_relations(
        &self,
        tx: &dyn WriteTx,
        appended: &[usize],
    ) -> Result<(), PortError> {
        let mut known: BTreeMap<String, bool> = BTreeMap::new();
        let mut dangling = BTreeSet::new();
        for position in appended {
            for mutation in &self.mutations[*position] {
                let ProjectionMutation::UpsertNodeRelation(relation) = mutation else {
                    continue;
                };
                for endpoint in [&relation.source_node_id, &relation.target_node_id] {
                    if !self.is_known(tx, &mut known, endpoint)? {
                        dangling.insert(format!(
                            "`{}` -{}-> `{}` (missing `{endpoint}`)",
                            relation.source_node_id,
                            relation.relation_type,
                            relation.target_node_id
                        ));
                    }
                }
            }
        }
        if dangling.is_empty() {
            return Ok(());
        }
        Err(PortError::Conflict(format!(
            "import refused; nothing was written. {} relation{} would point at a node neither \
             the imported abouts nor this store hold; add the about that owns it with another \
             --about: {}",
            dangling.len(),
            if dangling.len() == 1 { "" } else { "s" },
            dangling.into_iter().collect::<Vec<_>>().join(", ")
        )))
    }

    fn is_known(
        &self,
        tx: &dyn WriteTx,
        known: &mut BTreeMap<String, bool>,
        node_id: &str,
    ) -> Result<bool, PortError> {
        if self.materialised.contains(node_id) {
            return Ok(true);
        }
        if let Some(answer) = known.get(node_id) {
            return Ok(*answer);
        }
        let answer = match tx.get(Table::Nodes, Key::Str(node_id))? {
            Some(raw) => {
                decode::<NodeRecord>("destination node", &raw)?
                    .into_projection()?
                    .node_kind
                    != PLACEHOLDER_KIND
            }
            None => false,
        };
        known.insert(node_id.to_string(), answer);
        Ok(answer)
    }
}
