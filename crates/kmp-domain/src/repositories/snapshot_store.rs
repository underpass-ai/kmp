use std::future::Future;
use std::sync::Arc;

use crate::{KmpBundle, PortError, SnapshotSaveOptions};

pub trait SnapshotStore {
    fn save_bundle_with_options(
        &self,
        bundle: &KmpBundle,
        options: SnapshotSaveOptions,
    ) -> impl Future<Output = Result<(), PortError>> + Send;

    fn save_bundle(
        &self,
        bundle: &KmpBundle,
    ) -> impl Future<Output = Result<(), PortError>> + Send {
        self.save_bundle_with_options(bundle, SnapshotSaveOptions::default())
    }
}

impl<T> SnapshotStore for Arc<T>
where
    T: SnapshotStore + Send + Sync + ?Sized,
{
    async fn save_bundle_with_options(
        &self,
        bundle: &KmpBundle,
        options: SnapshotSaveOptions,
    ) -> Result<(), PortError> {
        self.as_ref()
            .save_bundle_with_options(bundle, options)
            .await
    }
}

impl<T> SnapshotStore for &T
where
    T: SnapshotStore + Send + Sync + ?Sized,
{
    async fn save_bundle_with_options(
        &self,
        bundle: &KmpBundle,
        options: SnapshotSaveOptions,
    ) -> Result<(), PortError> {
        (*self).save_bundle_with_options(bundle, options).await
    }
}
