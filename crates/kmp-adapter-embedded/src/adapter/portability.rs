//! Export/import (E6): the append-only event log is the portable form of an
//! embedded store. Export dumps it in sequence order; import replays it into
//! an empty store, reproducing identical revisions, idempotency outcomes and
//! projections — temporal reads and relation proof survive the round trip by
//! construction.

use kmp_domain::{ContextEventStore, ContextUpdatedEvent, PortError, ProjectionMutation};
use serde::{Deserialize, Serialize};

use super::replay::ProjectionRebuildReport;
use super::store::EmbeddedKernelStore;

/// First line of a bundle file: integrity metadata for fail-fast import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleHeader {
    pub bundle_format: u32,
    pub store_format: u32,
    pub event_count: u64,
    pub kernel_version: String,
}

pub const BUNDLE_FORMAT_VERSION: u32 = 1;

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
        let header = BundleHeader {
            bundle_format: BUNDLE_FORMAT_VERSION,
            store_format: super::format_version::SUPPORTED_FORMAT_VERSION,
            event_count: events.len() as u64,
            kernel_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        let mut out = String::new();
        out.push_str(&encode_line("bundle header", &header)?);
        for event in &events {
            out.push_str(&encode_line("bundle event", event)?);
        }
        Ok(out)
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

        let mut lines = bundle.lines().filter(|line| !line.trim().is_empty());
        let header: BundleHeader = decode_line(
            "bundle header",
            lines.next().ok_or_else(|| {
                PortError::InvalidState("bundle is empty: missing header line".to_string())
            })?,
        )?;
        if header.bundle_format != BUNDLE_FORMAT_VERSION {
            return Err(PortError::InvalidState(format!(
                "bundle format {} is not supported (this binary reads {})",
                header.bundle_format, BUNDLE_FORMAT_VERSION
            )));
        }
        if header.store_format != super::format_version::SUPPORTED_FORMAT_VERSION {
            return Err(PortError::InvalidState(format!(
                "bundle was exported from store format {}, this binary supports {}",
                header.store_format,
                super::format_version::SUPPORTED_FORMAT_VERSION
            )));
        }

        let mut events = Vec::new();
        for line in lines {
            events.push(decode_line::<ContextUpdatedEvent>("bundle event", line)?);
        }
        let events_imported = self.replay_event_stream(events).await?;
        if events_imported != header.event_count {
            return Err(PortError::InvalidState(format!(
                "bundle header declares {} events but {} were present",
                header.event_count, events_imported
            )));
        }

        let rebuild = self.rebuild_projections(derive).await?;
        Ok(ImportReport {
            events_imported,
            rebuild,
        })
    }
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
