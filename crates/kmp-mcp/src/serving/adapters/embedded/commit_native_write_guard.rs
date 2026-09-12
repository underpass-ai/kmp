use crate::serving::{ToolError, ToolErrorCode};
use kmp_domain::PortError;
use kmp_embedded::{CommitNativeBundle, EmbeddedKernelStore, PendingBundleExport};
use serde_json::Value;

/// Owns one canonical ingest's bundle guard until publication or a known rejection.
/// Ambiguous failures deliberately retain the existing pending marker.
pub(crate) struct CommitNativeWriteGuard<'a> {
    bundle: &'a CommitNativeBundle,
    store: &'a EmbeddedKernelStore,
    pending: PendingBundleExport,
}

impl<'a> CommitNativeWriteGuard<'a> {
    pub(crate) async fn begin(
        bundle: &'a CommitNativeBundle,
        store: &'a EmbeddedKernelStore,
    ) -> Result<Self, ToolError> {
        let pending = bundle
            .begin_write(store)
            .await
            .map_err(Self::preflight_error)?;
        Ok(Self {
            bundle,
            store,
            pending,
        })
    }

    pub(crate) async fn finish(self, result: Result<Value, ToolError>) -> Result<Value, ToolError> {
        let Self {
            bundle: commit_native,
            store,
            pending,
        } = self;
        match result {
            Ok(result) => {
                let header = commit_native
                    .publish(store, &pending)
                    .await
                    .map_err(|error| {
                        ToolError::backend(format!(
                            "memory write committed, but the commit-native bundle `{}` did not: \
                         {error}. The pending marker remains; run `kmp-mcp export` before \
                         trusting or committing this memory.",
                            commit_native.path().display()
                        ))
                    })?;
                pending.complete().map_err(|error| {
                    ToolError::backend(format!(
                        "memory and bundle {} are current at snapshot {}, but the pending \
                         marker could not be cleared: {error}",
                        commit_native.path().display(),
                        header.snapshot_id
                    ))
                })?;
                Ok(result)
            }
            Err(error)
                if matches!(
                    error.code,
                    ToolErrorCode::InvalidArgument
                        | ToolErrorCode::NotFound
                        | ToolErrorCode::Conflict
                        | ToolErrorCode::UnknownTool
                ) =>
            {
                pending.complete().map_err(|marker_error| {
                    ToolError::backend(format!(
                        "write was rejected as {}, but its commit-native pending marker \
                         could not be cleared: {marker_error}",
                        error.code
                    ))
                })?;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    fn preflight_error(error: PortError) -> ToolError {
        let message = format!(
            "memory write was refused before changing the store because its committed bundle could \
             not be guarded: {error}"
        );
        match error {
            PortError::Conflict(_) => ToolError::conflict(message),
            PortError::Unavailable(_) => ToolError::unavailable(message),
            PortError::InvalidState(_) => ToolError::backend(message),
        }
    }
}
