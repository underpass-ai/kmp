use kmp_domain::{
    AuthorNodeCard, NodeCard, NodeCardEvent, NodeCardStore, NodeCardWriteFuture, PortError,
    node_card_policy,
};

use super::engine::{Key, ReadTx, Table, WriteTx};
use super::serdes::{AggregateRecord, CardRecord, NodeRecord, decode, encode};
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
                // The header, never the record. A reader already paid to read
                // the body it is writing about; the kernel checking that write
                // must not pay for it a second time.
                let descriptor =
                    super::node_body_descriptor::read_one(tx.as_ref(), &command.node_id)?;
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

                append_event(tx.as_mut(), &command.about, &admitted, false)?;
                project(tx.as_mut(), &admitted)?;
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

/// Both live writes and replay use exactly the same card projection.
pub(super) fn project(tx: &mut dyn WriteTx, card: &NodeCard) -> Result<(), PortError> {
    if let Some(previous) = stored_card(tx, &card.node_id, &card.language)?
        && previous != *card
        && previous.card_revision.checked_add(1) != Some(card.card_revision)
    {
        return Err(PortError::InvalidState(
            "non-contiguous card projection".into(),
        ));
    }
    let bytes = encode("node card", &CardRecord::from(card.clone()))?;
    let revision = version_key(card)?;
    let key = Key::Str3(&card.node_id, &card.language, &revision);
    if let Some(previous) = tx.get(Table::CardVersions, key)?
        && previous != bytes
    {
        return Err(PortError::InvalidState(
            "immutable card revision differs".into(),
        ));
    }
    tx.insert(Table::CardVersions, key, &bytes)?;
    tx.insert(
        Table::Cards,
        Key::Str2(&card.node_id, &card.language),
        &bytes,
    )
}

pub(super) fn append_event(
    tx: &mut dyn WriteTx,
    about: &str,
    card: &NodeCard,
    baseline: bool,
) -> Result<(), PortError> {
    let event = NodeCardEvent::record(about, card, baseline, std::time::SystemTime::now())?;
    NodeCardEvent::card(&event)?;
    let key = super::store::aggregate_key(about, NodeCardEvent::ROLE);
    let revision = tx
        .get(Table::Aggregates, Key::Str(&key))?
        .map(|raw| decode::<AggregateRecord>("card stream head", &raw).map(|head| head.revision))
        .transpose()?
        .unwrap_or(0);
    super::context_events::append_in_transaction(tx, event, revision)?;
    Ok(())
}

/// A historical read selects the latest authored revision available at the cut.
/// If none existed yet, keep the current metadata so presentation reports
/// `after_cut` without disclosing its prose.
pub(super) fn read_batch_at(
    tx: &dyn ReadTx,
    ids: &[String],
    language: &str,
    cut: Option<i128>,
) -> Result<Vec<Option<NodeCard>>, PortError> {
    let Some(cut) = cut else {
        return read_batch(tx, ids, language);
    };
    let bound = history_key(cut, u64::MAX);
    ids.iter()
        .map(|id| {
            let selected = tx
                .last_str3_before(Table::CardVersions, id, language, &bound)?
                .map(|raw| decode::<CardRecord>("card revision", &raw).map(Into::into))
                .transpose()?;
            match selected {
                Some(card) => Ok(Some(card)),
                None => tx
                    .get(Table::Cards, Key::Str2(id, language))?
                    .map(|raw| decode::<CardRecord>("node card", &raw).map(Into::into))
                    .transpose(),
            }
        })
        .collect()
}

/// Flipping the sign bit gives fixed-width lexical ordering over signed
/// nanoseconds. The revision breaks ties when two authors share an instant.
fn history_key(nanos: i128, revision: u64) -> String {
    let ordered = (nanos as u128) ^ (1u128 << 127);
    format!("{ordered:039}:{revision:020}")
}

pub(super) fn version_key(card: &NodeCard) -> Result<String, PortError> {
    let nanos = kmp_domain::temporal_instant_nanos(&card.authored_at)
        .ok_or_else(|| PortError::InvalidState("invalid card authorship instant".into()))?;
    Ok(history_key(nanos, card.card_revision))
}
