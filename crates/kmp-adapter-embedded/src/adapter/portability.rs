//! Export/import (E6): the append-only event log is the portable form of an
//! embedded store. Export dumps it in sequence order; import replays it into
//! an empty store, reproducing identical revisions, idempotency outcomes and
//! projections — temporal reads and relation proof survive the round trip by
//! construction.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kmp_domain::{ContextEventStore, ContextUpdatedEvent, PortError, ProjectionMutation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::replay::ProjectionRebuildReport;
use super::store::EmbeddedKernelStore;

/// Inclusive positions in the exported event stream. A full-store snapshot
/// starts at one; an empty snapshot has neither bound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleEventRange {
    pub first: Option<u64>,
    pub last: Option<u64>,
}

/// First line of a bundle file: identity and integrity metadata for fail-fast
/// import. Fields added in bundle format 2 default only so format-1 bundles
/// remain readable; every format-2 field is validated before replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleHeader {
    pub bundle_format: u32,
    /// Format of the portable event payload, not the on-disk redb/SQLite
    /// layout. `store_format` is accepted from format-1 bundles because that
    /// older name described the field ambiguously.
    #[serde(rename = "event_format", alias = "store_format")]
    pub event_format: u32,
    pub event_count: u64,
    pub kernel_version: String,
    #[serde(default)]
    pub snapshot_id: String,
    #[serde(default)]
    pub created_at_unix_ms: u64,
    #[serde(default)]
    pub event_range: BundleEventRange,
    #[serde(default)]
    pub abouts: Vec<String>,
    #[serde(default)]
    pub content_digest: String,
}

pub const BUNDLE_FORMAT_VERSION: u32 = 2;

/// Outcome of an import: events replayed and projections rebuilt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportReport {
    pub events_imported: u64,
    pub rebuild: ProjectionRebuildReport,
}

impl EmbeddedKernelStore {
    /// Serializes the full event log as a JSON-Lines bundle: one header line
    /// followed by one event per line, in sequence order.
    pub async fn export_bundle(&self) -> Result<String, PortError> {
        let events = self.run(EmbeddedKernelStore::read_event_log).await?;
        encode_bundle(&events, None)
    }

    /// Exports the same complete stream with a human-selected snapshot id.
    /// The id is metadata, not a filename: callers may store the bundle in git,
    /// an artifact store, or anywhere else without changing what it identifies.
    pub async fn export_named_bundle(&self, snapshot_id: &str) -> Result<String, PortError> {
        if snapshot_id.trim().is_empty() {
            return Err(PortError::InvalidState(
                "snapshot id must not be empty".to_string(),
            ));
        }
        let events = self.run(EmbeddedKernelStore::read_event_log).await?;
        encode_bundle(&events, Some(snapshot_id))
    }

    /// Replays a bundle into this store. Fail-fast rules: the store must be
    /// empty (no merge semantics in v1 — ADR-011 rationale applies), the
    /// header must match supported formats, and every event must reproduce
    /// exactly the revision it was exported with.
    pub async fn import_bundle<F>(&self, bundle: &str, derive: F) -> Result<ImportReport, PortError>
    where
        F: Fn(&ContextUpdatedEvent) -> Result<Vec<ProjectionMutation>, PortError> + Send + 'static,
    {
        let (log_length, _) = self.event_log_stats().await?;
        if log_length != 0 {
            return Err(PortError::Conflict(format!(
                "import requires an empty store; this store already holds {log_length} events \
                 (merging bundles is not supported)"
            )));
        }

        let verified = parse_bundle(bundle)?;
        let header = verified.header;
        let events = verified.events;
        validate_revisions(&events)?;
        // Projection derivation is pure. Prove every payload is rebuildable
        // before the first append so a malformed later line cannot leave a
        // half-restored store behind.
        for event in &events {
            derive(event)?;
        }
        let events_imported = self.replay_event_stream(events).await?;
        debug_assert_eq!(events_imported, header.event_count);

        let rebuild = self.rebuild_projections(derive).await?;
        Ok(ImportReport {
            events_imported,
            rebuild,
        })
    }
}

/// Validates a bundle without opening or mutating a store. This is the
/// recovery check: identity, range, about coverage and digest are all proved
/// before an operator trusts a saved copy.
pub fn verify_bundle(bundle: &str) -> Result<BundleHeader, PortError> {
    parse_bundle(bundle).map(|verified| verified.header)
}

/// Merges only histories that have a deterministic answer: identical streams
/// or one exact prefix of the other. Two branches that both appended at the
/// same position are a semantic conflict, so KMP refuses to invent an order.
pub fn merge_bundles(left: &str, right: &str, snapshot_id: &str) -> Result<String, PortError> {
    if snapshot_id.trim().is_empty() {
        return Err(PortError::InvalidState(
            "merged snapshot id must not be empty".to_string(),
        ));
    }
    let left = parse_bundle(left)?;
    let right = parse_bundle(right)?;
    let shared = left.events.len().min(right.events.len());
    if let Some(position) =
        (0..shared).find(|position| left.events[*position] != right.events[*position])
    {
        return Err(PortError::Conflict(format!(
            "bundle histories diverge at event position {}; KMP only fast-forwards an exact \
             prefix and will not invent causal order for two branches",
            position + 1
        )));
    }
    let events = if left.events.len() >= right.events.len() {
        left.events
    } else {
        right.events
    };
    encode_bundle(&events, Some(snapshot_id))
}

struct VerifiedBundle {
    header: BundleHeader,
    events: Vec<ContextUpdatedEvent>,
}

fn parse_bundle(bundle: &str) -> Result<VerifiedBundle, PortError> {
    let mut lines = bundle.lines().filter(|line| !line.trim().is_empty());
    let header: BundleHeader = decode_line(
        "bundle header",
        lines.next().ok_or_else(|| {
            PortError::InvalidState("bundle is empty: missing header line".to_string())
        })?,
    )?;
    if !matches!(header.bundle_format, 1 | BUNDLE_FORMAT_VERSION) {
        return Err(PortError::InvalidState(format!(
            "bundle format {} is not supported (this binary reads 1 and {})",
            header.bundle_format, BUNDLE_FORMAT_VERSION
        )));
    }
    if header.event_format != super::format_version::EVENT_FORMAT_VERSION {
        return Err(PortError::InvalidState(format!(
            "bundle carries event format {}, this binary supports {}",
            header.event_format,
            super::format_version::EVENT_FORMAT_VERSION
        )));
    }

    let mut events = Vec::new();
    let mut event_payload = String::new();
    for line in lines {
        events.push(decode_line::<ContextUpdatedEvent>("bundle event", line)?);
        event_payload.push_str(line);
        event_payload.push('\n');
    }
    if events.len() as u64 != header.event_count {
        return Err(PortError::InvalidState(format!(
            "bundle header declares {} events but {} were present",
            header.event_count,
            events.len()
        )));
    }

    if header.bundle_format == BUNDLE_FORMAT_VERSION {
        validate_v2_header(&header, &events, &event_payload)?;
    }
    Ok(VerifiedBundle { header, events })
}

fn validate_v2_header(
    header: &BundleHeader,
    events: &[ContextUpdatedEvent],
    event_payload: &str,
) -> Result<(), PortError> {
    if header.snapshot_id.trim().is_empty() {
        return Err(PortError::InvalidState(
            "bundle format 2 requires snapshot_id".to_string(),
        ));
    }
    if header.created_at_unix_ms == 0 {
        return Err(PortError::InvalidState(
            "bundle format 2 requires created_at_unix_ms".to_string(),
        ));
    }
    let expected_range = event_range(events.len());
    if header.event_range != expected_range {
        return Err(PortError::InvalidState(format!(
            "bundle event_range {:?} does not cover its {} events (expected {:?})",
            header.event_range,
            events.len(),
            expected_range
        )));
    }
    let expected_abouts = abouts(events);
    if header.abouts != expected_abouts {
        return Err(PortError::InvalidState(format!(
            "bundle abouts do not match its events (expected {})",
            expected_abouts.join(", ")
        )));
    }
    let expected_digest = content_digest(event_payload.as_bytes());
    if header.content_digest != expected_digest {
        return Err(PortError::InvalidState(format!(
            "bundle content digest mismatch: header says {}, events produce {expected_digest}",
            header.content_digest
        )));
    }
    Ok(())
}

fn encode_bundle(
    events: &[ContextUpdatedEvent],
    snapshot_id: Option<&str>,
) -> Result<String, PortError> {
    let mut event_payload = String::new();
    for event in events {
        event_payload.push_str(&encode_line("bundle event", event)?);
    }
    let digest = content_digest(event_payload.as_bytes());
    let named = snapshot_id.is_some();
    let snapshot_id = snapshot_id
        .map(str::to_string)
        .unwrap_or_else(|| format!("content-{}", &digest[7..23]));
    // Content-addressed head exports must be byte-identical across redb,
    // SQLite, repeated exports and migrations. Their creation coordinate is
    // therefore the newest event time. A named recovery point records when
    // the operator created that point.
    let created_at = if named {
        SystemTime::now()
    } else {
        events
            .iter()
            .map(|event| event.occurred_at)
            .max()
            .unwrap_or(UNIX_EPOCH + Duration::from_millis(1))
    };
    let created_at_unix_ms = created_at
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64;
    let header = BundleHeader {
        bundle_format: BUNDLE_FORMAT_VERSION,
        event_format: super::format_version::EVENT_FORMAT_VERSION,
        event_count: events.len() as u64,
        kernel_version: env!("CARGO_PKG_VERSION").to_string(),
        snapshot_id,
        created_at_unix_ms,
        event_range: event_range(events.len()),
        abouts: abouts(events),
        content_digest: digest,
    };
    let mut out = encode_line("bundle header", &header)?;
    out.push_str(&event_payload);
    Ok(out)
}

fn event_range(event_count: usize) -> BundleEventRange {
    if event_count == 0 {
        BundleEventRange::default()
    } else {
        BundleEventRange {
            first: Some(1),
            last: Some(event_count as u64),
        }
    }
}

fn abouts(events: &[ContextUpdatedEvent]) -> Vec<String> {
    events
        .iter()
        .map(|event| event.root_node_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn content_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn validate_revisions(events: &[ContextUpdatedEvent]) -> Result<(), PortError> {
    let mut revisions: BTreeMap<(&str, &str), u64> = BTreeMap::new();
    for (position, event) in events.iter().enumerate() {
        let previous = revisions
            .get(&(event.root_node_id.as_str(), event.role.as_str()))
            .copied()
            .unwrap_or(0);
        let expected = previous + 1;
        if event.revision != expected {
            return Err(PortError::InvalidState(format!(
                "bundle event position {} carries revision {} for ({}, {}), expected {}; no \
                 events were imported",
                position + 1,
                event.revision,
                event.root_node_id,
                event.role,
                expected
            )));
        }
        revisions.insert(
            (event.root_node_id.as_str(), event.role.as_str()),
            event.revision,
        );
    }
    Ok(())
}

impl EmbeddedKernelStore {
    /// Replays a history into this store, in order, checking that every
    /// event lands on the revision it was recorded with.
    ///
    /// That check is the whole point: a replay that silently renumbers
    /// history would produce a store that reads plausibly and cites
    /// revisions that never existed. Shared by import and migration, which
    /// are the same operation seen from two different distances.
    pub(crate) async fn replay_event_stream<I>(&self, events: I) -> Result<u64, PortError>
    where
        I: IntoIterator<Item = ContextUpdatedEvent>,
    {
        let mut replayed = 0u64;
        for event in events {
            let recorded_revision = event.revision;
            let expected_previous = recorded_revision.checked_sub(1).ok_or_else(|| {
                PortError::InvalidState("event carries revision 0; the log is corrupt".to_string())
            })?;
            let assigned = self.append(event, expected_previous).await?;
            if assigned != recorded_revision {
                return Err(PortError::Conflict(format!(
                    "replay integrity violation: assigned revision {assigned}, \
                     history recorded {recorded_revision}"
                )));
            }
            replayed += 1;
        }
        Ok(replayed)
    }
}

fn encode_line<T: Serialize>(what: &str, value: &T) -> Result<String, PortError> {
    let mut line = serde_json::to_string(value)
        .map_err(|error| PortError::InvalidState(format!("could not encode {what}: {error}")))?;
    line.push('\n');
    Ok(line)
}

fn decode_line<T: for<'de> Deserialize<'de>>(what: &str, line: &str) -> Result<T, PortError> {
    serde_json::from_str(line)
        .map_err(|error| PortError::InvalidState(format!("could not decode {what}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(root: &str, revision: u64, content_hash: &str) -> ContextUpdatedEvent {
        ContextUpdatedEvent {
            root_node_id: root.to_string(),
            role: "agent".to_string(),
            revision,
            content_hash: content_hash.to_string(),
            changes: Vec::new(),
            idempotency_key: Some(format!("{root}:{revision}")),
            logical_digest: None,
            requested_by: Some("portability-test".to_string()),
            occurred_at: UNIX_EPOCH + Duration::from_secs(revision),
        }
    }

    #[test]
    fn format_two_identifies_and_covers_the_snapshot() {
        let events = vec![event("project:b", 1, "b"), event("project:a", 1, "a")];
        let bundle = encode_bundle(&events, Some("pre-release")).expect("bundle");
        let header = verify_bundle(&bundle).expect("verified");

        assert_eq!(header.bundle_format, BUNDLE_FORMAT_VERSION);
        assert_eq!(
            header.event_format,
            super::super::format_version::EVENT_FORMAT_VERSION
        );
        assert_eq!(header.snapshot_id, "pre-release");
        assert!(header.created_at_unix_ms > 0);
        assert_eq!(
            header.event_range,
            BundleEventRange {
                first: Some(1),
                last: Some(2),
            }
        );
        assert_eq!(header.abouts, ["project:a", "project:b"]);
        assert!(header.content_digest.starts_with("sha256:"));
    }

    #[test]
    fn tampering_is_rejected_before_a_bundle_can_be_replayed() {
        let bundle =
            encode_bundle(&[event("project:a", 1, "before")], Some("saved")).expect("bundle");
        let tampered = bundle.replace("\"content_hash\":\"before\"", "\"content_hash\":\"after\"");
        let error = verify_bundle(&tampered).expect_err("digest catches changed payload");
        assert!(error.to_string().contains("content digest mismatch"));
    }

    #[test]
    fn merge_fast_forwards_an_exact_prefix() {
        let first = event("project:a", 1, "one");
        let second = event("project:a", 2, "two");
        let left = encode_bundle(std::slice::from_ref(&first), Some("left")).expect("left");
        let right = encode_bundle(&[first, second], Some("right")).expect("right");

        let merged = merge_bundles(&left, &right, "merged").expect("fast forward");
        let header = verify_bundle(&merged).expect("verified merge");
        assert_eq!(header.snapshot_id, "merged");
        assert_eq!(header.event_count, 2);
    }

    #[test]
    fn merge_refuses_two_histories_at_the_same_position() {
        let left = encode_bundle(&[event("project:a", 1, "left")], Some("left")).expect("left");
        let right = encode_bundle(&[event("project:a", 1, "right")], Some("right")).expect("right");

        let error = merge_bundles(&left, &right, "invented").expect_err("must refuse");
        assert!(error.to_string().contains("diverge at event position 1"));
        assert!(error.to_string().contains("will not invent causal order"));
    }

    #[test]
    fn legacy_format_one_remains_readable() {
        let legacy =
            r#"{"bundle_format":1,"store_format":1,"event_count":0,"kernel_version":"0.1.3"}"#;
        let header = verify_bundle(legacy).expect("format one remains portable");
        assert_eq!(header.event_format, 1);
        assert!(header.snapshot_id.is_empty());
    }

    #[test]
    fn invalid_later_revision_is_rejected_in_preflight() {
        let events = [event("project:a", 1, "one"), event("project:a", 3, "three")];
        let error = validate_revisions(&events).expect_err("revision gap");
        assert!(error.to_string().contains("position 2"));
        assert!(error.to_string().contains("no events were imported"));
    }
}
