//! The decision half of `import --from --about`: compare the source's event
//! streams with the destination's and say what to append, or refuse.
//!
//! Pure — it reads two event lists and touches no store — so the whole
//! verdict exists before the first write.

use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{ContextUpdatedEvent, PortError};

use super::about_import_outcome::AboutImportOutcome;

/// One event stream: an aggregate is `(about, role)`, exactly as the store
/// keys its heads.
type StreamKey<'e> = (&'e str, &'e str);

/// What importing the source's events of the requested abouts into the
/// destination means, stream by stream.
///
/// A stream absent from the destination is replayed whole. A stream the
/// destination holds with the same revisions and content hashes is left
/// alone. A destination stream that is an exact prefix of the source's is
/// extended with the missing tail: appending revision `n + 1` to a stream at
/// head `n` is exactly what every live write does, so the projections the
/// tail produces are the ones a live session would have produced. Anything
/// else — a differing revision or hash, or a destination that is ahead — is
/// a different history, and the whole import is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AboutImportPlan {
    outcomes: BTreeMap<String, AboutImportOutcome>,
    /// Positions in the source event list to append, in source order.
    appended: Vec<usize>,
}

impl AboutImportPlan {
    /// Compares `source` (already restricted to the requested abouts, in
    /// source order, revisions validated) with the destination's log.
    /// `destination` may hold any abouts; only the requested ones count.
    pub(crate) fn compare(
        requested: &[String],
        source: &[ContextUpdatedEvent],
        destination: &[ContextUpdatedEvent],
    ) -> Result<Self, PortError> {
        let requested = requested
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let source_streams = streams(source, &requested);
        let destination_streams = streams(destination, &requested);

        let mut refusals = Vec::new();
        let mut heads: BTreeMap<StreamKey<'_>, u64> = BTreeMap::new();
        let mut per_about: BTreeMap<&str, Vec<StreamVerdict>> = BTreeMap::new();
        for about in &requested {
            per_about.insert(about, Vec::new());
        }

        for (key, held) in &destination_streams {
            if !source_streams.contains_key(key) {
                refusals.push(format!(
                    "`{}` (role `{}`): the destination is ahead — it holds revisions 1..={} \
                     that the source does not have",
                    key.0,
                    key.1,
                    held.len()
                ));
            }
        }
        for (key, offered) in &source_streams {
            let held = destination_streams
                .get(key)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            match first_divergence(offered, held) {
                Some(revision) => refusals.push(format!(
                    "`{}` (role `{}`): histories diverge at revision {revision}",
                    key.0, key.1
                )),
                None if held.len() > offered.len() => refusals.push(format!(
                    "`{}` (role `{}`): the destination is ahead at revision {} (source ends at \
                     {})",
                    key.0,
                    key.1,
                    offered.len() + 1,
                    offered.len()
                )),
                None => {
                    heads.insert(*key, held.len() as u64);
                    let verdict = if held.is_empty() {
                        StreamVerdict::Absent
                    } else if held.len() == offered.len() {
                        StreamVerdict::Identical
                    } else {
                        StreamVerdict::Prefix
                    };
                    per_about.entry(key.0).or_default().push(verdict);
                }
            }
        }
        if !refusals.is_empty() {
            return Err(PortError::Conflict(format!(
                "import refused; nothing was written. The destination already holds a different \
                 history for {}",
                refusals.join("; ")
            )));
        }

        let appended = source
            .iter()
            .enumerate()
            .filter(|(_, event)| {
                heads
                    .get(&(event.root_node_id.as_str(), event.role.as_str()))
                    .is_some_and(|head| event.revision > *head)
            })
            .map(|(position, _)| position)
            .collect();
        let outcomes = per_about
            .into_iter()
            .map(|(about, verdicts)| (about.to_string(), outcome(&verdicts)))
            .collect();
        Ok(Self { outcomes, appended })
    }

    /// Source positions to append, in source order.
    pub(crate) fn appended(&self) -> &[usize] {
        &self.appended
    }

    pub(crate) fn into_outcomes(self) -> BTreeMap<String, AboutImportOutcome> {
        self.outcomes
    }
}

/// How one stream of the source relates to the destination's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamVerdict {
    Absent,
    Identical,
    Prefix,
}

fn outcome(verdicts: &[StreamVerdict]) -> AboutImportOutcome {
    if verdicts
        .iter()
        .all(|verdict| *verdict == StreamVerdict::Absent)
    {
        AboutImportOutcome::Imported
    } else if verdicts
        .iter()
        .all(|verdict| *verdict == StreamVerdict::Identical)
    {
        AboutImportOutcome::Unchanged
    } else {
        AboutImportOutcome::Extended
    }
}

fn streams<'e>(
    events: &'e [ContextUpdatedEvent],
    requested: &BTreeSet<&str>,
) -> BTreeMap<StreamKey<'e>, Vec<&'e ContextUpdatedEvent>> {
    let mut streams: BTreeMap<StreamKey<'e>, Vec<&'e ContextUpdatedEvent>> = BTreeMap::new();
    for event in events {
        if requested.contains(event.root_node_id.as_str()) {
            streams
                .entry((event.root_node_id.as_str(), event.role.as_str()))
                .or_default()
                .push(event);
        }
    }
    streams
}

/// The first revision at which two streams of one aggregate disagree, over
/// the revisions both hold. Same revision and same content hash is the same
/// event: that is what the store itself compares heads by.
fn first_divergence(
    offered: &[&ContextUpdatedEvent],
    held: &[&ContextUpdatedEvent],
) -> Option<u64> {
    offered
        .iter()
        .zip(held)
        .find(|(offered, held)| {
            offered.revision != held.revision || offered.content_hash != held.content_hash
        })
        .map(|(_, held)| held.revision)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn event(root: &str, role: &str, revision: u64, hash: &str) -> ContextUpdatedEvent {
        ContextUpdatedEvent {
            root_node_id: root.to_string(),
            role: role.to_string(),
            revision,
            content_hash: hash.to_string(),
            changes: Vec::new(),
            idempotency_key: None,
            logical_digest: None,
            requested_by: None,
            occurred_at: UNIX_EPOCH + Duration::from_secs(revision),
        }
    }

    fn requested(abouts: &[&str]) -> Vec<String> {
        abouts.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn absent_identical_and_prefix_streams_each_get_their_outcome() {
        let source = vec![
            event("a", "agent", 1, "a1"),
            event("b", "agent", 1, "b1"),
            event("c", "agent", 1, "c1"),
            event("c", "agent", 2, "c2"),
        ];
        let destination = vec![
            event("other", "agent", 1, "x"),
            event("b", "agent", 1, "b1"),
            event("c", "agent", 1, "c1"),
        ];
        let plan = AboutImportPlan::compare(&requested(&["a", "b", "c"]), &source, &destination)
            .expect("no divergence");
        assert_eq!(plan.appended(), [0, 3]);
        assert_eq!(
            plan.into_outcomes(),
            BTreeMap::from([
                ("a".to_string(), AboutImportOutcome::Imported),
                ("b".to_string(), AboutImportOutcome::Unchanged),
                ("c".to_string(), AboutImportOutcome::Extended),
            ])
        );
    }

    #[test]
    fn a_differing_hash_names_the_about_role_and_first_diverging_revision() {
        let source = vec![
            event("a", "agent", 1, "a1"),
            event("a", "agent", 2, "a2"),
            event("ok", "agent", 1, "o1"),
        ];
        let destination = vec![event("a", "agent", 1, "a1"), event("a", "agent", 2, "zz")];
        let error = AboutImportPlan::compare(&requested(&["a", "ok"]), &source, &destination)
            .expect_err("diverging history is refused");
        let message = error.to_string();
        assert!(message.contains("`a` (role `agent`)"), "{message}");
        assert!(message.contains("diverge at revision 2"), "{message}");
        assert!(message.contains("nothing was written"), "{message}");
    }

    #[test]
    fn a_destination_ahead_is_refused() {
        let source = vec![event("a", "agent", 1, "a1")];
        let destination = vec![event("a", "agent", 1, "a1"), event("a", "agent", 2, "a2")];
        let error = AboutImportPlan::compare(&requested(&["a"]), &source, &destination)
            .expect_err("destination ahead");
        assert!(error.to_string().contains("ahead at revision 2"), "{error}");
    }

    #[test]
    fn a_destination_stream_the_source_lacks_is_ahead_too() {
        let source = vec![event("a", "agent", 1, "a1")];
        let destination = vec![event("a", "agent", 1, "a1"), event("a", "card", 1, "k1")];
        let error = AboutImportPlan::compare(&requested(&["a"]), &source, &destination)
            .expect_err("extra destination stream");
        assert!(error.to_string().contains("role `card`"), "{error}");
    }

    #[test]
    fn abouts_are_matched_exactly() {
        let source = vec![event("a", "agent", 1, "a1")];
        let destination = vec![event("a ", "agent", 1, "zz"), event("A", "agent", 1, "zz")];
        let plan = AboutImportPlan::compare(&requested(&["a"]), &source, &destination)
            .expect("near-miss abouts are other abouts");
        assert_eq!(plan.appended(), [0]);
    }
}
