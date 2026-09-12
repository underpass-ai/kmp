use kmp_domain::{
    AuthorNodeCard, NodeBodyDescriptor, NodeCard, NodeCardStore, NodeCardWriteFuture, PortError,
    node_card_policy,
};
use sha2::{Digest, Sha256};

use super::engine::{Key, ReadTx, Table, WriteTx};
use super::serdes::{CardRecord, DetailRecord, NodeRecord, decode, encode};
use super::store::EmbeddedKernelStore;

/// Cards in requested order, preserving duplicates and missing slots.
///
/// Takes the caller's transaction, so a trace presents cards from the same
/// snapshot it read descriptors and adjacency from. A card read against a
/// later snapshot could describe a body this response never showed.
pub(super) fn read_batch(
    tx: &dyn ReadTx,
    node_ids: &[String],
    language: &str,
) -> Result<Vec<Option<NodeCard>>, PortError> {
    node_ids
        .iter()
        .map(|id| {
            tx.get(Table::Cards, Key::Str2(id, language))?
                .map(|raw| decode::<CardRecord>("node card", &raw).map(Into::into))
                .transpose()
        })
        .collect()
}

/// `sha256:<lowercase hex>` over the exact stored record bytes.
///
/// The same definition the header table will carry, so a card authored before
/// that table exists stays valid after it does. It is computed over the bytes
/// as stored, never over the decoded text: escaping and field order are part
/// of what changed when a record changed.
pub(super) fn record_digest(raw: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(raw))
}

/// The descriptor of one body, derived from its stored record.
///
/// Authorship is the one path that legitimately reads a canonical record: the
/// reader had to read the body to write about it. Compact reads never come
/// here; they take descriptors from the port, which reads no record at all.
fn descriptor_from_record(node_id: &str, raw: &[u8]) -> Result<NodeBodyDescriptor, PortError> {
    let record: DetailRecord = decode("card body", raw)?;
    let detail = kmp_domain::NodeDetailProjection::from(record);
    Ok(NodeBodyDescriptor {
        node_id: node_id.to_string(),
        revision: detail.revision,
        content_hash: detail.content_hash,
        record_bytes: raw.len() as u64,
        body_bytes: detail.detail.len() as u64,
        record_digest: record_digest(raw),
    })
}

fn stored_card(
    tx: &dyn WriteTx,
    node_id: &str,
    language: &str,
) -> Result<Option<NodeCard>, PortError> {
    tx.get(Table::Cards, Key::Str2(node_id, language))?
        .map(|raw| decode::<CardRecord>("node card", &raw).map(Into::into))
        .transpose()
}

impl NodeCardStore for EmbeddedKernelStore {
    fn author_node_card(&self, command: AuthorNodeCard) -> NodeCardWriteFuture<'_> {
        Box::pin(async move {
            self.run(move |store| {
                // One write transaction for the whole decision. The node, its
                // body descriptor and the stored card are read here, inside
                // the lock the write already holds, so nothing can move
                // between the check and the insert — which is the only thing
                // that makes the declared source version mean anything.
                let mut tx = store.begin_write()?;
                let node = tx
                    .get(Table::Nodes, Key::Str(&command.node_id))?
                    .map(|raw| decode::<NodeRecord>("card node", &raw)?.into_projection())
                    .transpose()?;
                let descriptor = tx
                    .get(Table::Details, Key::Str(&command.node_id))?
                    .map(|raw| descriptor_from_record(&command.node_id, &raw))
                    .transpose()?;
                let existing = stored_card(tx.as_ref(), &command.node_id, &command.language)?;

                let admitted = match node_card_policy::admit(
                    &command,
                    node.as_ref(),
                    descriptor.as_ref(),
                    existing.as_ref(),
                    None,
                ) {
                    Ok(card) => card,
                    // Dropping the transaction discards it: a refused card
                    // leaves the store exactly as it found it.
                    Err(rejection) => return Ok(Err(rejection)),
                };

                let record = CardRecord::from(admitted.clone());
                let encoded = encode("node card", &record)?;
                tx.insert(
                    Table::Cards,
                    Key::Str2(&admitted.node_id, &admitted.language),
                    &encoded,
                )?;
                tx.commit()?;
                Ok(Ok(admitted))
            })
            .await
        })
    }
}

#[cfg(test)]
#[path = "node_card_tests.rs"]
mod tests;
