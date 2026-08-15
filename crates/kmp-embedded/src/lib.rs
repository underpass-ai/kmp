//! Embedded edition composition root (ADR-013): wires the application
//! services over the redb store with synchronous in-process projection, and
//! resolves the data directory per the ADR-012 contract. No transport, no
//! infrastructure clients.

mod data_dir;
mod kernel;
mod memory_api;
mod migration;

pub use data_dir::{DATA_DIR_ENV, ResolvedDataDir, resolve_data_dir, resolve_data_dir_from_env};
pub use kernel::{EmbeddedKernel, EmbeddedMemoryService};
pub use kmp_adapter_embedded::{SUPPORTED_FORMAT_VERSION, StoreMigrationReceipt};
pub use migration::{migrate_data_dir, open_or_migrate_data_dir};
