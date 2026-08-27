//! Embedded edition composition root (ADR-013): wires the application
//! services over the local store with synchronous in-process projection, and
//! resolves the data directory per the ADR-012 contract and the storage
//! engine per ADR-018. No transport, no infrastructure clients.

mod commit_bundle;
mod data_dir;
mod engine;
mod kernel;
mod memory_api;
mod migration;

pub use commit_bundle::{
    CommitNativeBundle, PENDING_EXPORT_DIR, PendingBundleExport, clear_pending_bundle_exports,
    pending_bundle_exports, write_bundle_atomically, write_bundle_if_absent,
};
pub use data_dir::{
    DATA_DIR_ENV, PROJECT_BUNDLE_PATH, ResolvedDataDir, ensure_data_dir_skeleton,
    locate_data_dir_from_env, project_bundle_path, resolve_data_dir, resolve_data_dir_from_env,
    user_data_home,
};
pub use engine::{
    ENGINE_ENV, default_engine_for_data_dir, parse_engine, resolve_engine_for_data_dir_from_env,
    resolve_engine_from_env,
};
pub use kernel::{EmbeddedKernel, EmbeddedMemoryService};
pub use kmp_adapter_embedded::{
    BUNDLE_FORMAT_VERSION, BundleEventRange, BundleHeader, EmbeddedKernelStore,
    RedbQualityTelemetryReader, SUPPORTED_FORMAT_VERSION, StorageEngine, StoreMigrationReceipt,
    format_version_path, merge_bundles, quality_telemetry_path, read_stamped_version,
    store_file_path_for, verify_bundle,
};
pub use migration::{migrate_data_dir, migrate_data_dir_to, open_or_migrate_data_dir};
