use kmp_domain::ReadSnapshotProvider;

use super::store::EmbeddedKernelStore;

impl ReadSnapshotProvider<EmbeddedKernelStore, EmbeddedKernelStore> for EmbeddedKernelStore {
    fn open_snapshot(
        &self,
    ) -> kmp_domain::ReadSnapshotFuture<'_, EmbeddedKernelStore, EmbeddedKernelStore> {
        Box::pin(async {
            let snapshot = self.read_snapshot().await?;
            Ok((snapshot.clone(), snapshot))
        })
    }
}
