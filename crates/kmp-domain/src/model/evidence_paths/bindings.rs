use super::EvidenceMissingWitness;
use std::collections::{BTreeMap, BTreeSet};

/// Intersections of present witnesses, alongside missing witnesses never filled
/// from another path. Independent domains do not model tuple correlations.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct EvidencePathBindings {
    pub domains: BTreeMap<String, BTreeSet<String>>,
    pub missing: BTreeSet<EvidenceMissingWitness>,
}

impl EvidencePathBindings {
    pub fn restrict(&mut self, name: &str, values: &BTreeSet<String>) -> bool {
        let domain = self
            .domains
            .entry(name.into())
            .or_insert_with(|| values.clone());
        domain.retain(|v| values.contains(v));
        !domain.is_empty()
    }

    pub fn joined(&self, other: &Self) -> Option<Self> {
        let mut joined = self.clone();
        for (name, values) in &other.domains {
            if !joined.restrict(name, values) {
                return None;
            }
        }
        joined.missing.extend(other.missing.iter().cloned());
        Some(joined)
    }
}
