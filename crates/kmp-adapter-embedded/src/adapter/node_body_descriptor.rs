//! The shared descriptor, built from the storage-owned header.
//!
//! This is the seam the admission contract names: `detail_header` owns what a
//! header is and how it is validated, and this turns a validated header into
//! the domain type every reader works from. No path here reads a `Details`
//! value, which is what makes a valid card reusable with zero canonical reads.

use kmp_domain::{NodeBodyDescriptor, PortError};

use super::detail_header;
use super::engine::ReadTx;

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
