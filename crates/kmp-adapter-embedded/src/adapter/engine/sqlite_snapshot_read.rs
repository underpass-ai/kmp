use std::sync::MutexGuard;

use kmp_domain::PortError;
use rusqlite::Connection;

use super::sqlite::Ops;
use super::{Key, ReadTx, Str3Row, StrRow, Table, U64Row};

pub(super) struct SqliteSnapshotRead<'a>(pub(super) MutexGuard<'a, Option<Connection>>);

impl SqliteSnapshotRead<'_> {
    fn ops(&self) -> Ops<'_> {
        Ops {
            connection: self.0.as_ref().expect("snapshot connection"),
        }
    }
}

impl ReadTx for SqliteSnapshotRead<'_> {
    fn get(&self, table: Table, key: Key<'_>) -> Result<Option<Vec<u8>>, PortError> {
        self.ops().get(table, key)
    }
    fn scan_str(&self, table: Table) -> Result<Vec<StrRow>, PortError> {
        self.ops().scan_str(table)
    }
    fn scan_str3_by_first(&self, table: Table, first: &str) -> Result<Vec<Str3Row>, PortError> {
        self.ops().scan_str3_by_first(table, first)
    }
    fn scan_u64(&self, table: Table) -> Result<Vec<U64Row>, PortError> {
        self.ops().scan_u64(table)
    }
    fn last_u64(&self, table: Table) -> Result<Option<U64Row>, PortError> {
        self.ops().last_u64(table)
    }
    fn count(&self, table: Table) -> Result<u64, PortError> {
        self.ops().count(table)
    }
}
