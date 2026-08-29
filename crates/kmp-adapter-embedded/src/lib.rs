//! Embedded edition storage adapters ([historical ADR-009](https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/docs/adr/ADR-009-embedded-storage-engine.md)).
//!
//! One [`EmbeddedKernelStore`] opens one data directory
//! ([historical ADR-012](https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/docs/adr/ADR-012-embedded-data-directory.md) layout:
//! `FORMAT_VERSION` plus an engine-named file under `store/`) and implements every persistence
//! port the kernel needs: graph reads (materialized adjacency per
//! [historical ADR-010](https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/docs/adr/ADR-010-embedded-graph-representation.md)),
//! node details, the append-only context event log, projection runtime state,
//! and snapshots. SQLite is the only active engine and is fsync-durable, so
//! the crash contract is: no data loss beyond the in-flight event, no duplicate
//! application on replay. Format-1 stores are detected and rejected without
//! being touched; this crate contains no redb reader.
//!
//! The observable semantics are pinned by `kmp-conformance`: the same
//! suite that certifies the in-memory kernel store and the Neo4j/Valkey
//! adapters runs against this store.

mod adapter;

pub use adapter::{
    BUNDLE_FORMAT_VERSION, BundleEventRange, BundleHeader, EVENT_FORMAT_VERSION,
    EmbeddedKernelStore, ImportReport, LEGACY_REDB_FORMAT_VERSION, ProjectionRebuildReport,
    QualityTelemetryRetention, SUPPORTED_FORMAT_VERSION, SqliteQualityTelemetryReader,
    SqliteQualityTelemetryWriter, StorageEngine, StoreMigrationReceipt, format_version_path,
    legacy_quality_telemetry_path, legacy_redb_store_path, merge_bundles, quality_telemetry_path,
    read_stamped_version, store_file_path_for, validate_store_layout, verify_bundle,
};
