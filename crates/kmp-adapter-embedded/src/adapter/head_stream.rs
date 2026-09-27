use std::collections::BTreeSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kmp_domain::{ContextUpdatedEvent, PortError};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::portability::{BUNDLE_FORMAT_VERSION, BundleEventRange, BundleHeader};

/// A bundle's event stream kept open at its end, so a write can extend the
/// head export by the events it added instead of exporting the whole log
/// again (DESIGN L6, write in O(delta)).
///
/// Every export goes through here: a stream extended event by event encodes
/// to the same bytes as one built from the whole log at once, and so to the
/// same content-addressed head bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeadStream {
    payload: String,
    event_count: u64,
    abouts: BTreeSet<String>,
    /// The newest event time: a content-addressed head's creation
    /// coordinate.
    newest: Option<SystemTime>,
    carries_cards: bool,
}

impl HeadStream {
    /// A stream holding `events`, in order.
    pub fn of<'a>(
        events: impl IntoIterator<Item = &'a ContextUpdatedEvent>,
    ) -> Result<Self, PortError> {
        let mut stream = Self::default();
        stream.extend(events)?;
        Ok(stream)
    }

    /// Appends `events`, in order, after those the stream holds.
    pub fn extend<'a>(
        &mut self,
        events: impl IntoIterator<Item = &'a ContextUpdatedEvent>,
    ) -> Result<(), PortError> {
        for event in events {
            self.payload.push_str(&encode_line("bundle event", event)?);
            self.event_count += 1;
            if !self.abouts.contains(&event.root_node_id) {
                self.abouts.insert(event.root_node_id.clone());
            }
            self.newest = Some(match self.newest {
                Some(newest) => newest.max(event.occurred_at),
                None => event.occurred_at,
            });
            self.carries_cards |= event.role == kmp_domain::NodeCardEvent::ROLE;
        }
        Ok(())
    }

    pub fn event_count(&self) -> u64 {
        self.event_count
    }

    /// The content-addressed head bundle: its snapshot id and creation time
    /// are read from its events, so equal streams give equal bytes.
    pub fn encode(&self) -> Result<String, PortError> {
        self.encode_as(None)
    }

    fn header_as(&self, snapshot_id: Option<&str>) -> BundleHeader {
        let digest = content_digest(self.payload.as_bytes());
        let named = snapshot_id.is_some();
        let snapshot_id = snapshot_id
            .map(str::to_string)
            .unwrap_or_else(|| format!("content-{}", &digest[7..23]));
        // Content-addressed head exports must be byte-identical across
        // storage layouts and repeated exports. Their creation coordinate is
        // therefore the newest event time. A named recovery point records
        // when the operator created that point.
        let created_at = if named {
            SystemTime::now()
        } else {
            self.newest.unwrap_or(UNIX_EPOCH + Duration::from_millis(1))
        };
        let created_at_unix_ms = created_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64;
        BundleHeader {
            bundle_format: BUNDLE_FORMAT_VERSION,
            event_format: if self.carries_cards {
                super::format_version::EVENT_FORMAT_VERSION
            } else {
                2
            },
            event_count: self.event_count,
            kernel_version: env!("CARGO_PKG_VERSION").to_string(),
            snapshot_id,
            created_at_unix_ms,
            event_range: event_range(self.event_count),
            abouts: self.abouts.iter().cloned().collect(),
            content_digest: digest,
        }
    }

    /// The content-addressed head bundle and its header, hashed once.
    pub fn encode_with_header(&self) -> Result<(String, BundleHeader), PortError> {
        let header = self.header_as(None);
        let mut out = encode_line("bundle header", &header)?;
        out.push_str(&self.payload);
        Ok((out, header))
    }

    /// The bundle, content-addressed or, with `snapshot_id`, a named
    /// recovery point created now.
    pub(super) fn encode_as(&self, snapshot_id: Option<&str>) -> Result<String, PortError> {
        let header = self.header_as(snapshot_id);
        let mut out = encode_line("bundle header", &header)?;
        out.push_str(&self.payload);
        Ok(out)
    }
}

pub(super) fn event_range(event_count: u64) -> BundleEventRange {
    if event_count == 0 {
        BundleEventRange::default()
    } else {
        BundleEventRange {
            first: Some(1),
            last: Some(event_count),
        }
    }
}

pub(super) fn content_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

pub(super) fn encode_line<T: Serialize>(what: &str, value: &T) -> Result<String, PortError> {
    let mut line = serde_json::to_string(value)
        .map_err(|error| PortError::InvalidState(format!("could not encode {what}: {error}")))?;
    line.push('\n');
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::portability::verify_bundle;

    fn event(root: &str, revision: u64, role: &str, secs: u64) -> ContextUpdatedEvent {
        ContextUpdatedEvent {
            root_node_id: root.to_string(),
            role: role.to_string(),
            revision,
            content_hash: format!("{root}:{revision}"),
            changes: Vec::new(),
            idempotency_key: Some(format!("{root}:{revision}")),
            logical_digest: None,
            requested_by: Some("head-stream-test".to_string()),
            occurred_at: UNIX_EPOCH + Duration::from_secs(secs),
        }
    }

    #[test]
    fn a_stream_extended_event_by_event_encodes_as_one_built_at_once() {
        let events = vec![
            event("project:b", 1, "agent", 30),
            event("project:a", 1, "agent", 10),
            event("project:b", 2, "agent", 20),
            event("project:a", 1, kmp_domain::NodeCardEvent::ROLE, 5),
        ];
        let whole = HeadStream::of(&events).expect("stream");
        for cut in 0..=events.len() {
            let mut extended = HeadStream::of(&events[..cut]).expect("head");
            extended.extend(&events[cut..]).expect("tail");
            assert_eq!(extended, whole, "cut at {cut}");
            assert_eq!(
                extended.encode().expect("bundle"),
                whole.encode().expect("bundle")
            );
        }
        let bundle = HeadStream::of(&events[..3])
            .expect("stream")
            .encode()
            .expect("bundle");
        let header = verify_bundle(&bundle).expect("verified");
        assert_eq!(header.event_count, 3);
        assert_eq!(header.abouts, ["project:a", "project:b"]);
        assert_eq!(header.created_at_unix_ms, 30_000);
        assert_eq!(header.event_format, 2);
        // A card event moves the stream to the current event format.
        let carded = whole.encode().expect("bundle");
        let carded = serde_json::from_str::<BundleHeader>(carded.lines().next().expect("header"))
            .expect("header");
        assert_eq!(
            carded.event_format,
            super::super::format_version::EVENT_FORMAT_VERSION
        );
        assert!(header.snapshot_id.starts_with("content-"));
    }

    #[test]
    fn an_empty_stream_is_the_empty_head() {
        let header =
            verify_bundle(&HeadStream::default().encode().expect("bundle")).expect("verified");
        assert_eq!(header.event_count, 0);
        assert_eq!(header.event_range, BundleEventRange::default());
        assert_eq!(header.created_at_unix_ms, 1);
    }
}
