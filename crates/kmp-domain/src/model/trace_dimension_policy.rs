use crate::{DimensionScopeMode, DimensionSelection, DimensionSelectionMode, DomainError};

/// Hard admission and soft ordering remain different caller choices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceDimensionPolicy {
    pub required: Option<DimensionSelection>,
    pub preferred: Option<DimensionSelection>,
}

impl TraceDimensionPolicy {
    pub fn validate(&self) -> Result<(), DomainError> {
        for selection in self.required.iter().chain(self.preferred.iter()) {
            if selection.scope_mode() != DimensionScopeMode::CurrentAbout {
                return Err(DomainError::InvalidState(
                    "bounded trace dimensions require current_about scope".into(),
                ));
            }
            if selection.dimensions().len() > 64
                || selection.scope_ids().len() > 64
                || selection.selectors().len() > 16
                || selection.selectors().iter().any(|s| s.values().len() > 64)
            {
                return Err(DomainError::InvalidState("trace dimensions allow at most 64 kinds/scopes, 16 selectors and 64 values per selector".into()));
            }
            if selection.mode() != DimensionSelectionMode::All && selection.dimensions().is_empty()
            {
                return Err(DomainError::InvalidState(
                    "trace only/except requires dimension kinds".into(),
                ));
            }
        }
        if self
            .preferred
            .as_ref()
            .is_some_and(|s| !Self::constrained(s))
        {
            return Err(DomainError::InvalidState(
                "search.prefer_dimensions requires a kind, scope value or selector".into(),
            ));
        }
        Ok(())
    }

    pub fn constrained(selection: &DimensionSelection) -> bool {
        selection.mode() != DimensionSelectionMode::All
            || !selection.scope_ids().is_empty()
            || selection.has_selectors()
    }

    pub fn reads_coordinates(&self) -> bool {
        self.required.as_ref().is_some_and(Self::constrained) || self.preferred.is_some()
    }

    pub fn is_active(&self) -> bool {
        self.required.is_some() || self.preferred.is_some()
    }
}
