use super::EvidencePathRole;
use crate::{
    DomainError, TemporalCursor, TemporalSelection, TraceSearchLimits, temporal_instant_nanos,
};
use std::collections::{BTreeMap, BTreeSet};

/// Seed-based evidence discovery. All roles are required; alternative paths are
/// kept, with a shared work budget independent of any rendering/token budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePathRequest {
    pub about: String,
    pub from: String,
    pub roles: Vec<EvidencePathRole>,
    pub constants: BTreeMap<String, BTreeSet<String>>,
    pub temporal: TemporalSelection,
    pub limits: TraceSearchLimits,
}

impl EvidencePathRequest {
    pub fn validate(&self) -> Result<(), DomainError> {
        self.limits.validate()?;
        let invalid = |message: &str| DomainError::InvalidState(message.into());
        if self.about.trim().is_empty()
            || self.from.trim().is_empty()
            || self.roles.is_empty()
            || self.roles.len() > 8
        {
            return Err(invalid(
                "evidence paths require an about, a seed and 1..8 roles",
            ));
        }
        let mut names = BTreeSet::new();
        let mut kinds = BTreeMap::new();
        for role in &self.roles {
            if role.name.trim().is_empty()
                || !names.insert(&role.name)
                || role.steps.is_empty()
                || role.steps.len() > 1024
            {
                return Err(invalid(
                    "evidence roles require unique names and 1..1024 ordered steps",
                ));
            }
            for binding in &role.bindings {
                if (role.context && binding.at() == 0)
                    || binding.at() as usize > role.steps.len()
                    || binding.name().trim().is_empty()
                    || binding.label_key().is_some_and(|k| k.trim().is_empty())
                {
                    return Err(invalid(
                        "evidence obligation needs a valid path position, name and label key",
                    ));
                }
                if kinds
                    .insert(binding.name(), binding.label_key())
                    .is_some_and(|old| old != binding.label_key())
                {
                    return Err(invalid(
                        "an evidence binding cannot mix keys or labels and references",
                    ));
                }
            }
        }
        if self.constants.iter().any(|(name, values)| {
            !kinds.contains_key(name.as_str())
                || values.is_empty()
                || values.iter().any(|v| v.trim().is_empty())
        }) {
            return Err(invalid(
                "constants must constrain declared obligations with nonempty values",
            ));
        }
        match self.temporal.cursor() {
            Some(TemporalCursor::Time(at)) if temporal_instant_nanos(at).is_some() => {}
            Some(TemporalCursor::Ref(r)) if !r.trim().is_empty() => {}
            None => {}
            _ => {
                return Err(invalid(
                    "evidence path cut requires an instant or a nonempty ref",
                ));
            }
        }
        Ok(())
    }
}
