//! Backend-independent conformance suite for KMP storage semantics.
//!
//! The scenarios in this crate are the executable definition of what a KMP
//! storage backend must do. They are expressed exclusively in terms of the
//! domain ports and the application services — no transport, no adapter
//! types — so the same suite runs unchanged against:
//!
//! - the in-memory kernel store (`kmp-testkit::InMemoryKernelStore`),
//! - the containerized Neo4j/Valkey adapters (via `kmp-tests-shared`),
//! - future adapter sets (the embedded edition's redb stores).
//!
//! A backend passes conformance when every scenario completes without
//! panicking. Scenario isolation is per [`ConformanceBackendFactory::fresh`]
//! call: each scenario builds its own empty backend.

mod backend;
pub mod scenarios;

pub use backend::{
    BackendMemoryService, BackendProjectionService, BackendProjectionWriter, ConformanceBackend,
    ConformanceBackendFactory, FactoryBackend,
};
