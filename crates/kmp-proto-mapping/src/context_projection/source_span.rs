use serde::{Deserialize, Serialize};

/// Explicit host-declared provenance for one complete returned prose slot.
/// The source must be a returned record with exactly this text fingerprint.
/// Offsets are half-open UTF-8 bytes, never character or tokenizer positions.
/// Validation checks literal agreement, not the truth of the host's attribution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub packet: usize,
    pub pointer: String,
    pub source_ref: String,
    pub source_sha256: String,
    pub start_utf8: usize,
    pub end_utf8: usize,
}
