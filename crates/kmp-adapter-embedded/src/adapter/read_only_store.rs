//! Opening another workspace's memory to read it, never to change it.
//!
//! `import --from <store>` reads a store that belongs to somebody else: a
//! different project, a different session, maybe one that is live right now.
//! The ordinary open path is not allowed near it — it stamps fresh
//! directories, upgrades format stamps, adopts legacy cards and switches the
//! journal mode, and every one of those is a write.

use std::path::Path;
use std::sync::Arc;

use kmp_domain::PortError;

use super::engine::sqlite::SqliteEngine;
use super::engine::{Key, Table};
use super::format_version::{self, store_file_path_for};
use super::node_card_adoption::MARKER as CARD_ADOPTION_MARKER;
use super::store::EmbeddedKernelStore;

impl EmbeddedKernelStore {
    /// Opens the existing store in `data_dir` read-only.
    ///
    /// Nothing is created, stamped, migrated or adopted, and no writer lock
    /// is taken: every SQLite connection is opened read-only. A directory
    /// that is not a KMP store, a store this binary cannot read, and a store
    /// whose legacy cards were never moved into its event log are refused
    /// rather than read partially — exporting events from that last one would
    /// silently leave its cards behind.
    pub fn open_read_only(data_dir: &Path) -> Result<Self, PortError> {
        let engine = format_version::validate_store_layout(data_dir)?.ok_or_else(|| {
            PortError::InvalidState(format!(
                "`{}` is not a KMP store: it has no FORMAT_VERSION",
                data_dir.display()
            ))
        })?;
        let store_file = store_file_path_for(data_dir, engine);
        if !store_file.is_file() {
            return Err(PortError::InvalidState(format!(
                "`{}` is a KMP store that was never written: `{}` does not exist",
                data_dir.display(),
                store_file.display()
            )));
        }
        let store = Self::from_engine(Arc::new(SqliteEngine::open_read_only_file(&store_file)?));
        let read = store.begin_read()?;
        let adopted = read
            .get(Table::Migrations, Key::Str(CARD_ADOPTION_MARKER))?
            .is_some();
        if !adopted && read.count(Table::Cards)? > 0 {
            return Err(PortError::InvalidState(format!(
                "`{}` still keeps its cards outside the event log; open it once with this KMP \
                 version so they are adopted, then import from it",
                data_dir.display()
            )));
        }
        drop(read);
        Ok(store)
    }
}
