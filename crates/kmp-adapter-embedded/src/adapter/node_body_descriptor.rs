//! The shared descriptor, built from the storage-owned header.
//!
//! This is the seam the admission contract names: `detail_header` owns what a
//! header is and how it is validated, and this turns a validated header into
//! the domain type every reader works from. No path here reads a `Details`
//! value, which is what makes a valid card reusable with zero canonical reads.

use kmp_domain::{NodeBodyDescriptor, NodeDetailProjection, PortError};
use sha2::{Digest, Sha256};

use super::detail_header;
use super::engine::{Key, ReadTx, Table};
use super::serdes::{DetailRecord, decode};

pub(super) fn read_batch(
    tx: &dyn ReadTx,
    node_ids: &[String],
) -> Result<Vec<Option<NodeBodyDescriptor>>, PortError> {
    Ok(detail_header::read_batch(tx, node_ids)?
        .into_iter()
        .map(|header| {
            header.map(|header| NodeBodyDescriptor {
                node_id: header.node_id,
                revision: header.revision,
                content_hash: header.content_hash,
                record_bytes: header.record_bytes,
                body_bytes: header.body_bytes,
                record_digest: header.record_digest,
            })
        })
        .collect())
}

/// The descriptor of one body, for the write path that authors a card.
pub(super) fn read_one(
    tx: &dyn ReadTx,
    node_id: &str,
) -> Result<Option<NodeBodyDescriptor>, PortError> {
    Ok(read_batch(tx, std::slice::from_ref(&node_id.to_string()))?
        .pop()
        .flatten())
}

/// Admitted bodies, each checked against the digest its header records before
/// it is handed back.
///
/// The check happens here because here is the only place that holds both: the
/// exact bytes just read, and the digest written beside them in the same
/// transaction as the record itself. Anywhere further out the bytes are gone
/// and only the decoded projection survives, so a body whose text changed
/// under an unchanged public revision and content hash — the case the record
/// digest exists for — would pass every remaining comparison and be delivered
/// under an identity it no longer has.
///
/// The digest is taken over the stored bytes as read, never over a
/// reserialization of the decoded value: re-encoding would compare this
/// build's serializer against itself and agree with anything.
///
/// Only admitted ids reach this function, so nothing deferred is read.
pub(super) fn read_verified_bodies(
    tx: &dyn ReadTx,
    node_ids: &[String],
) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
    node_ids
        .iter()
        .map(|id| {
            let Some(raw) = tx.get(Table::Details, Key::Str(id))? else {
                return Ok(None);
            };
            let expected = read_one(tx, id)?.ok_or_else(|| {
                PortError::InvalidState(format!(
                    "embedded store: `{id}` has a stored body and no descriptor; this \
                     projection is inconsistent and the body is not delivered"
                ))
            })?;
            let actual = format!("sha256:{:x}", Sha256::digest(&raw));
            if actual != expected.record_digest {
                return Err(PortError::InvalidState(format!(
                    "embedded store: the body record of `{id}` does not match the digest its \
                     descriptor records ({} stored, {actual} read); it is not delivered under \
                     an identity it does not have",
                    expected.record_digest
                )));
            }
            Ok(Some(decode::<DetailRecord>("node detail", &raw)?.into()))
        })
        .collect()
}
