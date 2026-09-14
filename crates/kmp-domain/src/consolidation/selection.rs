use super::{ConsolidatedView, ConsolidationAxis, ConsolidationSource};
use crate::{PortError, temporal_instant_nanos};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsolidationSelection {
    pub axis: ConsolidationAxis,
    pub as_of: String,
}

impl ConsolidationSelection {
    /// Select only from the explicitly named source versions. Missing clocks
    /// do not acquire timestamps from the writer or view. An earlier cut than
    /// authorship cannot see the derived interpretation at all.
    pub fn eligible_sources(&self, view: &ConsolidatedView) -> Result<Vec<String>, PortError> {
        let cut = self.validate()?;
        let authored = temporal_instant_nanos(&view.authored_at)
            .ok_or_else(|| PortError::InvalidState("invalid view authorship clock".into()))?;
        if authored > cut {
            return Ok(vec![]);
        }
        Ok(view
            .sources
            .iter()
            .filter(|source| self.admits(source, cut))
            .map(|s| s.reference.clone())
            .collect())
    }

    pub fn validate(&self) -> Result<i128, PortError> {
        temporal_instant_nanos(&self.as_of)
            .ok_or_else(|| PortError::InvalidState("invalid consolidation as_of instant".into()))
    }

    fn admits(&self, source: &ConsolidationSource, cut: i128) -> bool {
        !source.coordinates.is_empty()
            && source
                .coordinates
                .iter()
                .chain(&source.dependency_clocks)
                .all(|clock| {
                    let start = match self.axis {
                        ConsolidationAxis::Occurred => &clock.occurred_at,
                        ConsolidationAxis::Observed => &clock.observed_at,
                        ConsolidationAxis::Ingested => &clock.ingested_at,
                        ConsolidationAxis::Validity => &clock.valid_from,
                    };
                    let Some(start) = start.as_deref().and_then(temporal_instant_nanos) else {
                        return false;
                    };
                    if start > cut {
                        return false;
                    }
                    if matches!(self.axis, ConsolidationAxis::Validity) {
                        clock.valid_until.as_deref().is_none_or(|end| {
                            temporal_instant_nanos(end).is_some_and(|end| end > cut)
                        })
                    } else {
                        true
                    }
                })
    }
}
