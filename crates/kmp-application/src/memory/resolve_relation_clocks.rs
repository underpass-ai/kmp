use super::{MemoryCoordinateData, MemoryRelationClocks};
use crate::ApplicationError;

/// Resolve new declarations from their own explicit clocks and packet provenance.
/// Entry clocks are deliberately not an input. Replay uses the original receipt.
pub(super) fn resolve_relation_clocks(
    clocks: Option<&MemoryRelationClocks>,
    coordinate: Option<&MemoryCoordinateData>,
    observed_at: Option<&str>,
    ingested_at: &str,
) -> Result<MemoryRelationClocks, ApplicationError> {
    let mut resolved = clocks.cloned().unwrap_or_default();
    // A caller can explicitly qualify a relation with a dimensional coordinate.
    // Preserve its authored clocks, refusing two incompatible declarations.
    for (field, value, other) in [
        (
            "occurred_at",
            &mut resolved.occurred_at,
            coordinate.and_then(|c| c.occurred_at.as_ref()),
        ),
        (
            "observed_at",
            &mut resolved.observed_at,
            coordinate.and_then(|c| c.observed_at.as_ref()),
        ),
        (
            "ingested_at",
            &mut resolved.ingested_at,
            coordinate.and_then(|c| c.ingested_at.as_ref()),
        ),
        (
            "valid_from",
            &mut resolved.valid_from,
            coordinate.and_then(|c| c.valid_from.as_ref()),
        ),
        (
            "valid_until",
            &mut resolved.valid_until,
            coordinate.and_then(|c| c.valid_until.as_ref()),
        ),
    ] {
        if let (Some(at), Some(other)) = (value.as_ref(), other)
            && kmp_domain::compare_temporal_instants(at, other) != Some(std::cmp::Ordering::Equal)
        {
            return Err(ApplicationError::Validation(format!(
                "relation clocks.{field} conflicts with its explicit coordinate.{field}"
            )));
        }
        if value.is_none() {
            *value = other.cloned();
        }
        if let Some(at) = value
            && kmp_domain::temporal_instant_nanos(at).is_none()
        {
            return Err(ApplicationError::Validation(format!(
                "relation clocks.{field} is not an instant"
            )));
        }
    }
    // A preserved ingestion clock marks a historical declaration: importing it
    // must not invent an observation it did not carry.
    if resolved.ingested_at.is_none() {
        resolved
            .observed_at
            .get_or_insert_with(|| observed_at.unwrap_or(ingested_at).to_owned());
    }
    resolved
        .ingested_at
        .get_or_insert_with(|| ingested_at.to_owned());
    if let Some(at) = &resolved.observed_at {
        match kmp_domain::compare_temporal_instants(at, ingested_at) {
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal) => {}
            _ => {
                return Err(ApplicationError::Validation(
                    "relation clocks.observed_at must be an instant no later than this ingestion"
                        .to_string(),
                ));
            }
        }
    }
    if let (Some(from), Some(until)) = (&resolved.valid_from, &resolved.valid_until)
        && kmp_domain::compare_temporal_instants(from, until) != Some(std::cmp::Ordering::Less)
    {
        return Err(ApplicationError::Validation(
            "relation clocks.valid_until must be after clocks.valid_from".to_string(),
        ));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    const OLD: &str = "2026-09-01T10:00:00Z";
    const NOW: &str = "2026-09-02T10:00:00Z";

    #[test]
    fn declaration_defaults_preserve_unknown_event_and_validity() {
        let clocks = resolve_relation_clocks(None, None, None, NOW).expect("valid clock fixture");
        assert_eq!(
            clocks,
            MemoryRelationClocks {
                observed_at: Some(NOW.into()),
                ingested_at: Some(NOW.into()),
                ..Default::default()
            }
        );
        let clocks =
            resolve_relation_clocks(None, None, Some(OLD), NOW).expect("valid clock fixture");
        assert_eq!(clocks.observed_at.as_deref(), Some(OLD));
        assert_eq!(clocks.ingested_at.as_deref(), Some(NOW));
    }

    #[test]
    fn historical_ingestion_does_not_invent_an_observation() {
        let original = MemoryRelationClocks {
            ingested_at: Some(OLD.into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_relation_clocks(Some(&original), None, Some(NOW), NOW)
                .expect("valid clock fixture"),
            original
        );
    }

    #[test]
    fn explicit_coordinate_and_clock_instants_must_agree() {
        let coordinate = MemoryCoordinateData {
            dimension: "task".into(),
            scope_id: "review".into(),
            occurred_at: Some(OLD.into()),
            observed_at: None,
            ingested_at: None,
            valid_from: None,
            valid_until: None,
            sequence: None,
            rank: None,
            metadata: Default::default(),
        };
        let mut clocks = MemoryRelationClocks {
            occurred_at: Some("2026-09-01T12:00:00+02:00".into()),
            ..Default::default()
        };
        assert!(resolve_relation_clocks(Some(&clocks), Some(&coordinate), None, NOW).is_ok());
        clocks.occurred_at = Some(NOW.into());
        assert!(
            resolve_relation_clocks(Some(&clocks), Some(&coordinate), None, NOW)
                .expect_err("conflicting explicit clocks")
                .to_string()
                .contains("conflicts")
        );
    }

    #[test]
    fn malformed_instants_future_observations_and_reversed_validity_are_rejected() {
        for clocks in [
            MemoryRelationClocks {
                occurred_at: Some("yesterday".into()),
                ..Default::default()
            },
            MemoryRelationClocks {
                observed_at: Some(NOW.into()),
                ..Default::default()
            },
            MemoryRelationClocks {
                valid_from: Some(NOW.into()),
                valid_until: Some(OLD.into()),
                ..Default::default()
            },
        ] {
            assert!(resolve_relation_clocks(Some(&clocks), None, None, OLD).is_err());
        }
    }
}
