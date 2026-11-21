//! Embedded edition composition root (ADR-013): wires the application
//! services over the local store with synchronous in-process projection, and
//! resolves the data directory per the ADR-012 contract and the storage
//! engine per ADR-018. No transport, no infrastructure clients.

mod bundle_file;
mod commit_bundle;
mod committed_tail;
#[cfg(test)]
mod committed_tail_tests;
mod data_dir;
mod engine;
mod file_stamp;
mod history_advice;
mod kernel;
mod log_tail;
mod memory_api;
pub mod memory_selection;
mod memory_selection_refusal;
mod migration;
mod open_reconciliation;
#[cfg(test)]
mod open_reconciliation_tests;
mod project_marker;
mod read_head;
mod tail_memo;
pub mod user_config_file;

pub use commit_bundle::{
    CommitNativeBundle, PENDING_EXPORT_DIR, PendingBundleExport, clear_pending_bundle_exports,
    pending_bundle_exports, write_bundle_atomically, write_bundle_if_absent,
};
pub use data_dir::{
    DATA_DIR_ENV, OrphanedProjectBundle, PROJECT_BUNDLE_PATH, ResolvedDataDir,
    data_dir_override_refusal, ensure_data_dir_skeleton, locate_data_dir_from_env,
    project_bundle_path, resolve_data_dir, resolve_data_dir_from_env, user_data_home,
};
pub use engine::default_engine_for_data_dir;
pub use history_advice::{OVERWRITE_COMMITTED_FLAG, diverged_advice, store_ahead_advice};
pub use kernel::{EmbeddedKernel, EmbeddedMemoryService};
pub use kmp_adapter_embedded::{
    AboutImportOutcome, AboutImportReport, BUNDLE_FORMAT_VERSION, BundleEventRange, BundleHeader,
    EmbeddedKernelStore, HistoryRelation, SUPPORTED_FORMAT_VERSION, SqliteQualityTelemetryReader,
    StorageEngine, StoreMigrationReceipt, bundle_excluding_abouts, compare_bundle_histories,
    format_version_path, merge_bundles, quality_telemetry_path, read_stamped_version,
    refuse_vanished_store_database, store_file_path_for, validate_store_layout, verify_bundle,
};
pub use memory_selection::SelectedMemory;
pub use memory_selection_refusal::SelectionRefusal;
pub use migration::{migrate_data_dir, migrate_data_dir_to, open_or_migrate_data_dir};
pub use open_reconciliation::OpenReconciliation;
