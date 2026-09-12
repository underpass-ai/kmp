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
/// detail whose header is absent or whose length disagrees with the stored
/// record is an inconsistent projection and fails here, because answering
/// `None` would report present evidence as missing and reading the body would
/// defeat the point of asking.
///
/// No path through this function reads the `Details` value.
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
            let header = decode::<DetailHeaderRecord>("node detail header", &raw)?;
            if header.record_bytes != stored {
                return Err(inconsistent(
                    id,
                    &format!(
                        "has a header of {} bytes against a stored record of {stored}",
                        header.record_bytes
                    ),
                ));
            }
            Ok(Some(header))
        })
        .collect()
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
