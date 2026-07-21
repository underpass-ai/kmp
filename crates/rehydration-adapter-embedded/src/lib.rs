//! Embedded edition storage adapters ([ADR-009](../../../docs/adr/ADR-009-embedded-storage-engine.md)).
//!
//! One [`EmbeddedKernelStore`] opens one data directory
//! ([ADR-012](../../../docs/adr/ADR-012-embedded-data-directory.md) layout:
//! `FORMAT_VERSION` + `store/kernel.redb`) and implements every persistence
//! port the kernel needs: graph reads (materialized adjacency per
//! [ADR-010](../../../docs/adr/ADR-010-embedded-graph-representation.md)),
//! node details, the append-only context event log, projection runtime state,
//! and snapshots. Commits are fsync-durable (redb immediate durability), so
//! the crash contract is: no data loss beyond the in-flight event, no
//! duplicate application on replay.
//!
//! The observable semantics are pinned by `rehydration-conformance`: the same
//! suite that certifies the in-memory kernel store and the Neo4j/Valkey
//! adapters runs against this store.

mod adapter;

pub use adapter::{
    EmbeddedKernelStore, ProjectionRebuildReport, SUPPORTED_FORMAT_VERSION, format_version_path,
};
