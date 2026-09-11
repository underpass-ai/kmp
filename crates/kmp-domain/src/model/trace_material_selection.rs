use crate::{DomainError, TraceProofRequirement};
use std::collections::BTreeSet;

/// Optional material policy applied after bounded candidate discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceMaterialSelection {
    pub max_nodes: u32,
    pub max_paths: u32,
    pub groups: Vec<TraceProofRequirement>,
}

impl TraceMaterialSelection {
    pub fn validate(&self, targets: &BTreeSet<String>) -> Result<(), DomainError> {
        if !(1..=4096).contains(&self.max_nodes)
            || !(1..=8).contains(&self.max_paths)
            || self.groups.len() > 8
            || targets.is_empty()
            || targets.len() > 8
        {
            return Err(DomainError::InvalidState("trace material requires 1..4096 nodes, 1..8 paths, at most 8 groups and 1..8 targets".into()));
        }
        for group in &self.groups {
            if !(1..=1000).contains(&group.weight)
                || group.alternatives.is_empty()
                || group.alternatives.len() > 8
                || group
                    .alternatives
                    .iter()
                    .any(|a| a.is_empty() || a.len() > 8 || !a.is_subset(targets))
            {
                return Err(DomainError::InvalidState("each trace group requires weight 1..1000 and 1..8 nonempty alternatives drawn from to".into()));
            }
        }
        Ok(())
    }

    pub fn requirements(&self, targets: &BTreeSet<String>) -> Vec<TraceProofRequirement> {
        if self.groups.is_empty() {
            targets
                .iter()
                .map(|target| TraceProofRequirement {
                    alternatives: vec![BTreeSet::from([target.clone()])],
                    weight: 1,
                })
                .collect()
        } else {
            self.groups.clone()
        }
    }
}
