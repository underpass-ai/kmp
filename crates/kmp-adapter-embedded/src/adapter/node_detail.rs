use kmp_domain::{NodeDetailProjection, NodeDetailReader, PortError};

use super::engine::{Key, Table};
use super::serdes::{DetailRecord, decode};
use super::store::EmbeddedKernelStore;

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
            let mut results = Vec::with_capacity(node_ids.len());
            for node_id in &node_ids {
                results.push(match tx.get(Table::Details, Key::Str(node_id))? {
                    Some(raw) => Some(decode::<DetailRecord>("node detail", &raw)?.into()),
                    None => None,
                });
            }
            Ok(results)
        })
        .await
    }
}
