use kmp_domain::RelationExplanation;

use super::scalars::timestamp_from_sort_or_rfc3339;

/// What one relation an ask reads contributes to its about's lifecycle
/// (DESIGN L6, P13): the latest instant it carries, the moment its entry
/// stops applying when it is a `contains_entry` edge that says so, and its
/// sequence, which orders the about's relations.
///
/// The lexical sidecar keeps one per relation, so an ask that reads only its
/// candidates still stands on the frontier and the expiries the whole about
/// declares, exactly as [`super::memory_lifecycle::MemoryLifecycle::read`]
/// reads them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelationClock {
    pub latest: Option<(i64, i32)>,
    pub valid_until: Option<String>,
    pub sequence: Option<u32>,
}

impl RelationClock {
    pub fn of(explanation: &RelationExplanation) -> Self {
        Self {
            latest: [
                instant(explanation.occurred_at()),
                instant(explanation.observed_at()),
                instant(explanation.ingested_at()),
                instant(explanation.valid_from()),
            ]
            .into_iter()
            .flatten()
            .max(),
            valid_until: explanation.valid_until().map(str::to_string),
            sequence: explanation.sequence(),
        }
    }

    /// The instant `valid_until` names, when it names one.
    pub fn valid_until_instant(&self) -> Option<(i64, i32)> {
        instant(self.valid_until.as_deref())
    }
}

fn instant(value: Option<&str>) -> Option<(i64, i32)> {
    timestamp_from_sort_or_rfc3339(value).map(|time| (time.seconds, time.nanos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_domain::RelationSemanticClass;

    #[test]
    fn a_relation_contributes_its_latest_instant_expiry_and_sequence() {
        let explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_valid_from("2026-09-01T10:00:00Z")
            .with_observed_at("2026-09-01T12:00:00Z")
            .with_valid_until("2026-09-01T11:00:00Z")
            .with_sequence(7);
        let clock = RelationClock::of(&explanation);
        assert_eq!(clock.latest, instant(Some("2026-09-01T12:00:00Z")));
        assert_eq!(clock.sequence, Some(7));
        assert_eq!(
            clock.valid_until_instant(),
            instant(Some("2026-09-01T11:00:00Z"))
        );
        let bare = RelationClock::of(&RelationExplanation::new(RelationSemanticClass::Structural));
        assert_eq!(bare, RelationClock::default());
        assert_eq!(bare.valid_until_instant(), None);
    }
}
