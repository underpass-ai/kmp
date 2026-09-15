use kmp_domain::{NodeCard, PortError};

use super::engine::{Key, Table};
use super::serdes::{CardRecord, NodeRecord, decode};
use super::store::EmbeddedKernelStore;

/// Upgrade the surviving pre-event cards atomically. Reopening is a no-op;
/// lost historical revisions cannot be inferred and are never manufactured.
pub(super) const MARKER: &str = "authored-card-events-v1";

pub(super) fn adopt(store: &EmbeddedKernelStore) -> Result<(), PortError> {
    if store
        .begin_read()?
        .get(Table::Migrations, Key::Str(MARKER))?
        .is_some()
    {
        return Ok(());
    }
    let mut tx = store.begin_write()?;
    // Another opener may have completed adoption while this one waited.
    if tx.get(Table::Migrations, Key::Str(MARKER))?.is_some() {
        return Ok(());
    }

    for ((node_id, language), raw) in tx.scan_str2(Table::Cards)? {
        let card: NodeCard = decode::<CardRecord>("legacy node card", &raw)?.into();
        if card.node_id != node_id || card.language != language {
            return Err(PortError::InvalidState(
                "legacy card identity mismatch".into(),
            ));
        }
        let revision = super::node_card::version_key(&card)?;
        if let Some(recorded) = tx.get(
            Table::CardVersions,
            Key::Str3(&node_id, &language, &revision),
        )? {
            let recorded: NodeCard = decode::<CardRecord>("recorded card", &recorded)?.into();
            if recorded != card {
                return Err(PortError::InvalidState(
                    "card differs from its immutable revision".into(),
                ));
            }
            continue;
        }
        let node = tx
            .get(Table::Nodes, Key::Str(&node_id))?
            .ok_or_else(|| PortError::InvalidState("legacy card has no source node".into()))?;
        let node = decode::<NodeRecord>("card source", &node)?.into_projection()?;
        let about = node
            .properties
            .get("memory_about")
            .ok_or_else(|| PortError::InvalidState("legacy card source has no about".into()))?;
        super::node_card::append_event(tx.as_mut(), about, &card, true)?;
        super::node_card::project(tx.as_mut(), &card)?;
    }
    tx.insert(Table::Migrations, Key::Str(MARKER), b"1")?;
    tx.commit()
}
