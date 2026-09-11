use crate::{DomainError, MemoryRelationType, RelationDirection, TraceSearchLimits};
use std::collections::BTreeSet;

/// Explicit destinations on one selected clock. No cross-about inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceSearchRequest {
    pub about: String,
    pub from: String,
    pub targets: BTreeSet<String>,
    pub direction: RelationDirection,
    pub relations: BTreeSet<String>,
    pub follow: Vec<crate::TraceRelationStep>,
    pub paths_per_target: u32,
    pub select: Option<crate::TraceMaterialSelection>,
    pub limits: TraceSearchLimits,
    pub temporal: crate::TemporalSelection,
}

impl TraceSearchRequest {
    pub fn validate(&self) -> Result<(), DomainError> {
        self.limits.validate()?;
        if let Some(policy) = &self.select {
            policy.validate(&self.targets)?;
        }
        if !(1..=8).contains(&self.paths_per_target) || self.follow.len() > 16 {
            return Err(DomainError::InvalidState(
                "trace search requires paths_per_target 1..8 and at most 16 follow moves".into(),
            ));
        }
        if !self.follow.is_empty()
            && (!self.relations.is_empty() || self.direction != RelationDirection::Outgoing)
        {
            return Err(DomainError::InvalidState(
                "search.follow replaces global direction and relations".into(),
            ));
        }
        let mut moves = BTreeSet::new();
        for step in &self.follow {
            if !moves.insert((
                step.relation.as_str(),
                step.direction == RelationDirection::Incoming,
            )) {
                return Err(DomainError::InvalidState(
                    "search.follow requires distinct moves".into(),
                ));
            }
        }
        if let Some(cursor) = self.temporal.cursor() {
            match cursor {
                crate::TemporalCursor::Ref(reference) if !reference.trim().is_empty() => {}
                crate::TemporalCursor::Time(at) if crate::temporal_instant_nanos(at).is_some() => {}
                _ => {
                    return Err(DomainError::InvalidState(
                        "trace as_of requires a nonempty ref or a valid instant, never a sequence"
                            .into(),
                    ));
                }
            }
        }
        if self.about.trim().is_empty()
            || self.from.trim().is_empty()
            || self.targets.is_empty()
            || self.targets.len() > 8
            || self.targets.iter().any(|s| s.trim().is_empty())
        {
            return Err(DomainError::InvalidState(
                "trace search requires an about, a source and 1..8 destinations".into(),
            ));
        }
        for relation in &self.relations {
            MemoryRelationType::new(relation)?;
        }
        Ok(())
    }
}
