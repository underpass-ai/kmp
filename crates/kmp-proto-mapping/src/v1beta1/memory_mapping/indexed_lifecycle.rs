use std::collections::BTreeMap;

use prost_types::Timestamp;

use super::scalars::timestamp_from_sort_or_rfc3339;

/// The whole about's lifecycle as the lexical sidecar holds it (DESIGN L6,
/// P13): its frontier, the latest instant any of its relations carries, and
/// every `contains_entry` edge that names a `valid_until`, each with the key
/// the about's relations are ordered by.
///
/// An ask that reads only its candidates cannot see the frontier or the
/// expiries elsewhere in the about; these answer both exactly as reading the
/// whole about would.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexedLifecycle {
    frontier: Option<(i64, i32)>,
    /// `(sequence, source, target, valid_until)`.
    expiring: Vec<(Option<u32>, String, String, String)>,
}

impl IndexedLifecycle {
    pub fn new(
        frontier: Option<(i64, i32)>,
        expiring: Vec<(Option<u32>, String, String, String)>,
    ) -> Self {
        Self { frontier, expiring }
    }

    pub(super) fn frontier(&self) -> Option<(i64, i32)> {
        self.frontier
    }

    /// The entries whose applicability ended before the frontier, in the
    /// order a bundle holds its relations (by sequence, then source and
    /// target), a later edge of one entry answering for it.
    pub(super) fn expired(&self) -> BTreeMap<String, Option<Timestamp>> {
        let Some(frontier) = self.frontier else {
            return BTreeMap::new();
        };
        let mut expiring = self.expiring.iter().collect::<Vec<_>>();
        expiring.sort_by(|left, right| {
            (left.0.unwrap_or(u32::MAX), &left.1, &left.2).cmp(&(
                right.0.unwrap_or(u32::MAX),
                &right.1,
                &right.2,
            ))
        });
        expiring
            .into_iter()
            .filter_map(|(_, _, target, valid_until)| {
                let time = timestamp_from_sort_or_rfc3339(Some(valid_until.as_str()))?;
                ((time.seconds, time.nanos) < frontier).then(|| {
                    (
                        target.clone(),
                        timestamp_from_sort_or_rfc3339(Some(valid_until.as_str())),
                    )
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expiring(sequence: u32, target: &str, until: &str) -> (Option<u32>, String, String, String) {
        (Some(sequence), "about".into(), target.into(), until.into())
    }

    #[test]
    fn only_what_ended_before_the_frontier_has_expired() {
        let at = |value: &str| {
            let time = timestamp_from_sort_or_rfc3339(Some(value)).expect("instant");
            (time.seconds, time.nanos)
        };
        let lifecycle = IndexedLifecycle::new(
            Some(at("2026-09-01T12:00:00Z")),
            vec![
                expiring(2, "tape", "2026-09-01T11:00:00Z"),
                expiring(1, "glycol", "2026-09-02T00:00:00Z"),
            ],
        );
        assert_eq!(lifecycle.frontier(), Some(at("2026-09-01T12:00:00Z")));
        let expired = lifecycle.expired();
        assert_eq!(expired.keys().collect::<Vec<_>>(), ["tape"]);
        assert_eq!(
            expired["tape"].as_ref().map(|time| time.seconds),
            Some(at("2026-09-01T11:00:00Z").0)
        );
        // Without a frontier nothing is read as expired.
        let unanchored =
            IndexedLifecycle::new(None, vec![expiring(1, "tape", "2020-01-01T00:00:00Z")]);
        assert!(unanchored.expired().is_empty());
    }
}
