use kmp_application::GetContextResult;
use kmp_domain::{GraphReadRevision, TemporalSelection};

/// The operation snapshot and admitted collection that own the statistics.
/// Question, policy, bridge and page are deliberately absent: they are applied
/// after collection statistics. Exact term-count equality additionally guards
/// changes to admission or normalization; no hashed term keys are used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LexicalIndexIdentity {
    revision: GraphReadRevision,
    about: String,
    scopes: Vec<String>,
    temporal: TemporalSelection,
}

impl LexicalIndexIdentity {
    pub(super) fn read(result: &GetContextResult, temporal: &TemporalSelection) -> Option<Self> {
        Some(Self {
            revision: result.read_revision.clone()?,
            about: result.bundle.root_node().node_id().to_owned(),
            scopes: result.requested_scopes.clone(),
            temporal: temporal.clone(),
        })
    }

    pub(super) fn retained_bytes(&self) -> usize {
        // Temporal selectors and the opaque revision are serialized only for
        // conservative cache admission, never for identity or ranking.
        256 + self.revision.as_str().len()
            + self.about.len()
            + self
                .scopes
                .iter()
                .map(|scope| 32 + scope.len())
                .sum::<usize>()
            + format!("{:?}", self.temporal).len()
    }
}
