use kmp_domain::{NodeDetailProjection, NodeDetailReader, PortError};

use super::engine::{Key, ReadTx, Table};
use super::serdes::{DetailRecord, decode};
use super::store::EmbeddedKernelStore;

pub(super) fn read_batch(
    tx: &dyn ReadTx,
    node_ids: &[String],
) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
    node_ids
        .iter()
        .map(|id| {
            tx.get(Table::Details, Key::Str(id))?
                .map(|raw| decode::<DetailRecord>("node detail", &raw).map(Into::into))
                .transpose()
        })
        .collect()
}

/// Stored record bytes for each id, in the requested order, duplicates and
/// absent slots preserved exactly as `read_batch` reports them. Nothing is
/// loaded or decoded: the answer counts the stored detail record, envelope
/// included, which is larger than the canonical body inside it. It bounds what
/// a later read would fetch from this table, not the memory a request holds.
pub(super) fn size_batch(
    tx: &dyn ReadTx,
    node_ids: &[String],
) -> Result<Vec<Option<u64>>, PortError> {
    node_ids
        .iter()
        .map(|id| tx.value_len(Table::Details, Key::Str(id)))
        .collect()
}

impl NodeDetailReader for EmbeddedKernelStore {
    async fn load_node_detail(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeDetailProjection>, PortError> {
        let node_id = node_id.to_string();
        self.run(move |store| {
            let tx = store.begin_read()?;
            match tx.get(Table::Details, Key::Str(&node_id))? {
                Some(raw) => Ok(Some(decode::<DetailRecord>("node detail", &raw)?.into())),
                None => Ok(None),
            }
        })
        .await
    }

    async fn load_node_details_batch(
        &self,
        node_ids: Vec<String>,
    ) -> Result<Vec<Option<NodeDetailProjection>>, PortError> {
        self.run(move |store| {
            let tx = store.begin_read()?;
            read_batch(tx.as_ref(), &node_ids)
        })
        .await
    }
}

#[cfg(test)]
#[path = "node_detail_size_tests.rs"]
mod size_tests;
