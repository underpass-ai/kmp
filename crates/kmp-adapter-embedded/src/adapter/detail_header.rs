use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use kmp_domain::{NodeDetailProjection, PortError};

use super::engine::{Key, ReadTx, Table, WriteTx};
use super::node_detail::size_batch;
use super::serdes::{decode, encode};

/// What a canonical body weighs and which exact bytes it is, without the body.
///
/// Storage-owned metadata, not a reader claim and not a card field. It is
/// written in the same transaction as the `Details` row it describes, so the
/// pair is consistent by construction: a detail whose header is absent is a
/// projection that predates this record, never a body that does not exist.
///
/// `content_hash` is copied literally from the detail projection. It is a
/// public token derived from the event hash and node identity, so a write can
/// change the text without changing it; `record_digest` is the value that
/// actually identifies these bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct DetailHeaderRecord {
    pub(super) node_id: String,
    pub(super) revision: u64,
    pub(super) content_hash: String,
    /// Bytes of the serialized `Details` value, envelope included.
    pub(super) record_bytes: u64,
    /// Canonical UTF-8 bytes of the body inside that record.
    pub(super) body_bytes: u64,
    /// `sha256:<lowercase hex>` over the exact serialized `Details` value.
    pub(super) record_digest: String,
}

impl DetailHeaderRecord {
    /// Built from the detail and the exact bytes about to be stored for it, at
    /// the one place those bytes are serialized.
    pub(super) fn describe(detail: &NodeDetailProjection, record: &[u8]) -> Self {
        Self {
            node_id: detail.node_id.clone(),
            revision: detail.revision,
            content_hash: detail.content_hash.clone(),
            record_bytes: record.len() as u64,
            body_bytes: detail.detail.len() as u64,
            record_digest: format!("sha256:{:x}", Sha256::digest(record)),
        }
    }
}

/// Writes the header beside its detail. Both rows belong to the caller's
/// transaction, so either both land or neither does.
pub(super) fn write(tx: &mut dyn WriteTx, header: &DetailHeaderRecord) -> Result<(), PortError> {
    let bytes = encode("node detail header", header)?;
    tx.insert(Table::DetailHeaders, Key::Str(&header.node_id), &bytes)
}

/// Headers for each id, in the requested order, duplicates and absent slots
/// preserved exactly as `node_detail::read_batch` reports them.
///
/// `None` means no `Details` row: the body genuinely does not exist. A stored
/// detail whose header is absent, unreadable or inconsistent is an
/// inconsistent projection and fails here, because answering `None` would
/// report present evidence as missing and reading the body would defeat the
/// point of asking.
///
/// No path through this function reads the `Details` value, which bounds what
/// can be caught. Everything checkable without the bytes is checked: the
/// header names the body that was asked for, its record length matches the
/// stored record, its canonical body cannot exceed the record containing it,
/// and its digest has the one shape this store writes. What cannot be caught
/// here is a well-formed digest that does not describe those bytes, or a
/// plausible but wrong `body_bytes`. Both would need the record. Writing the
/// pair in one transaction is what makes them right at the source; a consumer
/// that later loads a body checks it against this digest.
///
/// The port forwarding that turns these into the shared `NodeBodyDescriptor`
/// belongs to the admission contract patch; until it lands, this side of the
/// table is reached only by its tests.
#[allow(dead_code)]
pub(super) fn read_batch(
    tx: &dyn ReadTx,
    node_ids: &[String],
) -> Result<Vec<Option<DetailHeaderRecord>>, PortError> {
    let record_bytes = size_batch(tx, node_ids)?;
    node_ids
        .iter()
        .zip(record_bytes)
        .map(|(id, stored)| {
            let Some(stored) = stored else {
                return Ok(None);
            };
            let raw = tx
                .get(Table::DetailHeaders, Key::Str(id))?
                .ok_or_else(|| inconsistent(id, "has no stored header"))?;
            let header =
                decode::<DetailHeaderRecord>("node detail header", &raw).map_err(|error| {
                    inconsistent(id, &format!("has an unreadable header ({error})"))
                })?;
            validated(id, header, stored).map(Some)
        })
        .collect()
}

/// Every invariant a header can be held to without its record. Serde proves
/// the JSON has the right field types and nothing more: a header that names
/// another body, counts more canonical bytes than the record holding them, or
/// carries a digest this store could not have written is corrupt, however well
/// it parses. The public `content_hash` is an opaque token by contract and is
/// not inspected.
fn validated(
    id: &str,
    header: DetailHeaderRecord,
    stored: u64,
) -> Result<DetailHeaderRecord, PortError> {
    if header.node_id != id {
        return Err(inconsistent(
            id,
            &format!("has a header naming `{}`", header.node_id),
        ));
    }
    if header.record_bytes != stored {
        return Err(inconsistent(
            id,
            &format!(
                "has a header of {} bytes against a stored record of {stored}",
                header.record_bytes
            ),
        ));
    }
    if header.body_bytes > header.record_bytes {
        return Err(inconsistent(
            id,
            &format!(
                "has a header claiming {} canonical bytes inside a record of {}",
                header.body_bytes, header.record_bytes
            ),
        ));
    }
    if !is_record_digest(&header.record_digest) {
        return Err(inconsistent(
            id,
            &format!(
                "has a header whose digest `{}` is not sha256 followed by 64 lowercase hex digits",
                header.record_digest
            ),
        ));
    }
    Ok(header)
}

/// The one digest shape this store writes: `sha256:` and 64 lowercase hex
/// digits. Uppercase is rejected rather than normalized, because a value this
/// store never produces is evidence of something else having written it.
fn is_record_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|digit| digit.is_ascii_digit() || matches!(digit, b'a'..=b'f'))
    })
}

fn inconsistent(node_id: &str, what: &str) -> PortError {
    PortError::InvalidState(format!(
        "embedded store: canonical body `{node_id}` {what}; rebuild the projections before \
         reading bodies in bounded mode"
    ))
}

#[cfg(test)]
#[path = "detail_header_tests.rs"]
mod tests;
