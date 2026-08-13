use kmp_domain::{NodeDetailProjection, NodeDetailReader, PortError};

use super::serdes::{DetailRecord, decode};
use super::store::{DETAILS, EmbeddedKernelStore, storage_error, table_error};

impl NodeDetailReader for EmbeddedKernelStore {
    async fn load_node_detail(
        &self,
        node_id: &str,
    ) -> Result<Option<NodeDetailProjection>, PortError> {
        let node_id = node_id.to_string();
        self.run(move |store| {
            let tx = store.begin_read()?;
            let details = tx.open_table(DETAILS).map_err(table_error)?;
            match details.get(node_id.as_str()).map_err(storage_error)? {
                Some(guard) => Ok(Some(
                    decode::<DetailRecord>("node detail", guard.value())?.into(),
                )),
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
            let details = tx.open_table(DETAILS).map_err(table_error)?;
            let mut results = Vec::with_capacity(node_ids.len());
            for node_id in &node_ids {
                results.push(
                    match details.get(node_id.as_str()).map_err(storage_error)? {
                        Some(guard) => {
                            Some(decode::<DetailRecord>("node detail", guard.value())?.into())
                        }
                        None => None,
                    },
                );
            }
            Ok(results)
        })
        .await
    }
}
