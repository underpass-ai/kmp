//! Accepted declaration clocks for an evidence item's `supports` associations.
use kmp_domain::compare_temporal_instants;

use crate::ApplicationError;

/// Boundary data. Source `time` remains separate from the support declaration.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct EvidenceSupportClocks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingested_at: Option<String>,
}

impl EvidenceSupportClocks {
    pub(super) fn resolve(
        supplied: Option<&Self>,
        observation: Option<&str>,
        ingestion: &str,
    ) -> Result<Self, ApplicationError> {
        let mut clocks = supplied.cloned().unwrap_or_default();
        let restored = clocks.ingested_at.is_some();
        if !restored {
            clocks.ingested_at = Some(ingestion.to_owned());
            clocks
                .observed_at
                .get_or_insert_with(|| observation.unwrap_or(ingestion).to_owned());
        }
        for (key, value) in [
            ("observed_at", &clocks.observed_at),
            ("ingested_at", &clocks.ingested_at),
        ] {
            if let Some(value) = value
                && compare_temporal_instants(value, value).is_none()
            {
                return Err(ApplicationError::Validation(format!(
                    "memory.evidence[].support_clocks.{key} must be a valid timestamp"
                )));
            }
        }
        if let Some(observed) = &clocks.observed_at {
            for end in [Some(ingestion), clocks.ingested_at.as_deref()]
                .into_iter()
                .flatten()
            {
                if compare_temporal_instants(observed, end) == Some(std::cmp::Ordering::Greater) {
                    return Err(ApplicationError::Validation(
                        "memory.evidence[].support_clocks.observed_at cannot follow ingestion"
                            .to_owned(),
                    ));
                }
            }
        }
        Ok(clocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_and_future_support_observations_are_refused() {
        let ingestion = "2026-09-02T10:00:00Z";
        for value in ["not-a-date", "2026-09-02T10:00:01Z"] {
            assert!(EvidenceSupportClocks::resolve(None, Some(value), ingestion).is_err());
        }
        let reversed = EvidenceSupportClocks {
            observed_at: Some("2026-09-01T11:00:00Z".into()),
            ingested_at: Some("2026-09-01T10:00:00Z".into()),
        };
        assert!(EvidenceSupportClocks::resolve(Some(&reversed), None, ingestion).is_err());
    }
}
