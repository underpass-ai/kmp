use kmp_domain::{KmpBundle, PortError, SnapshotSaveOptions, SnapshotStore};

use super::engine::{Key, Table};
use super::store::EmbeddedKernelStore;

impl SnapshotStore for EmbeddedKernelStore {
    async fn save_bundle_with_options(
        &self,
        bundle: &KmpBundle,
        options: SnapshotSaveOptions,
    ) -> Result<(), PortError> {
        // The snapshot port is write-only; persist an auditable summary keyed
        // by (root, role) so a stored decision context can be accounted for
        // offline. Full bundle rendering stays above the ports.
        let root_node_id = bundle.root_node().node_id().to_string();
        let role = bundle.role().as_str().to_string();
        let record = serde_json::json!({
            "root_node_id": root_node_id,
            "role": role,
            "neighbor_node_ids": bundle
                .neighbor_nodes()
                .iter()
                .map(|node| node.node_id())
                .collect::<Vec<_>>(),
            "relationship_count": bundle.relationships().len(),
            "node_detail_count": bundle.node_details().len(),
            "ttl_seconds": options.ttl_seconds(),
        });
        let bytes = serde_json::to_vec(&record).map_err(|error| {
            PortError::InvalidState(format!(
                "embedded store could not encode snapshot summary: {error}"
            ))
        })?;

        self.run(move |store| {
            let mut tx = store.begin_write()?;
            tx.insert(Table::Snapshots, Key::Str2(&root_node_id, &role), &bytes)?;
            tx.commit()
        })
        .await
    }
}
