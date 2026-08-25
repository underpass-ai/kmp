//! Embedded edition storage adapters ([ADR-009](../../../archive/docs/adr/ADR-009-embedded-storage-engine.md)).
//!
//! One [`EmbeddedKernelStore`] opens one data directory
//! ([ADR-012](../../../archive/docs/adr/ADR-012-embedded-data-directory.md) layout:
//! `FORMAT_VERSION` plus an engine-named file under `store/`) and implements every persistence
//! port the kernel needs: graph reads (materialized adjacency per
//! [ADR-010](../../../archive/docs/adr/ADR-010-embedded-graph-representation.md)),
//! node details, the append-only context event log, projection runtime state,
//! and snapshots. SQLite and the redb compatibility engine are fsync-durable,
//! so the crash contract is: no data loss beyond the in-flight event, no
//! duplicate application on replay.
//!
//! The observable semantics are pinned by `kmp-conformance`: the same
//! suite that certifies the in-memory kernel store and the Neo4j/Valkey
//! adapters runs against this store.

mod adapter;

pub use adapter::{
    BUNDLE_FORMAT_VERSION, BundleEventRange, BundleHeader, EVENT_FORMAT_VERSION,
    EmbeddedKernelStore, ImportReport, ProjectionRebuildReport, QualityTelemetryRetention,
    RedbQualityTelemetryReader, RedbQualityTelemetryWriter, SUPPORTED_FORMAT_VERSION,
    StorageEngine, StoreMigrationReceipt, format_version_path, merge_bundles,
    quality_telemetry_path, read_stamped_version, store_file_path_for, verify_bundle,
};
