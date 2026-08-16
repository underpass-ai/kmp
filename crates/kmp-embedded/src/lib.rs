//! Embedded edition composition root (ADR-013): wires the application
//! services over the local store with synchronous in-process projection, and
//! resolves the data directory per the ADR-012 contract and the storage
//! engine per ADR-018. No transport, no infrastructure clients.

mod data_dir;
mod engine;
mod kernel;
mod memory_api;
mod migration;

pub use data_dir::{DATA_DIR_ENV, ResolvedDataDir, resolve_data_dir, resolve_data_dir_from_env};
pub use engine::{ENGINE_ENV, parse_engine, resolve_engine_from_env};
pub use kernel::{EmbeddedKernel, EmbeddedMemoryService};
pub use kmp_adapter_embedded::{SUPPORTED_FORMAT_VERSION, StorageEngine, StoreMigrationReceipt};
pub use migration::{migrate_data_dir, migrate_data_dir_to, open_or_migrate_data_dir};
