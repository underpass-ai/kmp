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
