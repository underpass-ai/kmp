//! One declared link, compiled: what the ingest carries for it.

use serde_json::Value;

/// A `connect_to` declaration compiled for the canonical ingest: the relation,
/// its name and quality for the acknowledgment, and the evidence node its
/// proof text becomes, when it has one.
pub(super) struct CompiledLink {
    pub(super) relation: Value,
    pub(super) name: String,
    pub(super) quality: Value,
    pub(super) evidence: Option<Value>,
}
