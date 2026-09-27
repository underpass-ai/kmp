use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use kmp_proto_mapping::v1beta1::{LanguageSignals, LexicalRow, Posting, PostingBlock};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::about_change::AboutChange;
use super::about_stats::AboutStats;
use super::node_state::NodeState;
use super::relation_key::RelationKey;
use super::row_writer::RowWriter;
use super::sidecar_meta::SidecarMeta;
use super::storage;

/// The lexical index on SQLite (DESIGN L6, option (a)): a sidecar file with
/// its own WAL beside the store, derived entirely from the store's event log
/// and rebuilt whenever it cannot be trusted. Binaries that predate it never
/// open it, and several processes may share it: every write is one
/// transaction that first checks the sidecar still stands where the writer
/// read it.
pub(super) struct SqliteLexicalSidecar {
    connection: Mutex<Connection>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS lex_stats(about TEXT PRIMARY KEY, stats TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS lex_node(
    about TEXT NOT NULL, node TEXT NOT NULL, state BLOB NOT NULL,
    PRIMARY KEY (about, node)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS lex_relation(
    about TEXT NOT NULL, source TEXT NOT NULL, target TEXT NOT NULL, type TEXT NOT NULL,
    signals BLOB NOT NULL, PRIMARY KEY (about, source, target, type)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS lex_fwd(
    about TEXT NOT NULL, doc TEXT NOT NULL, ordinal INTEGER NOT NULL,
    fingerprint INTEGER NOT NULL, row BLOB NOT NULL,
    PRIMARY KEY (about, doc)) WITHOUT ROWID;
CREATE UNIQUE INDEX IF NOT EXISTS lex_fwd_ordinal ON lex_fwd(about, ordinal);
CREATE TABLE IF NOT EXISTS lex_post(
    about TEXT NOT NULL, term TEXT NOT NULL, block INTEGER NOT NULL, postings BLOB NOT NULL,
    PRIMARY KEY (about, term, block)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS lex_key_df(
    about TEXT NOT NULL, term TEXT NOT NULL, content INTEGER NOT NULL, direct INTEGER NOT NULL,
    PRIMARY KEY (about, term)) WITHOUT ROWID;
";

const TABLES: [&str; 6] = [
    "lex_stats",
    "lex_node",
    "lex_relation",
    "lex_fwd",
    "lex_post",
    "lex_key_df",
];

impl SqliteLexicalSidecar {
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        let connection = Connection::open(path).map_err(storage)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(10))
            .map_err(storage)?;
        connection
            .query_row("PRAGMA journal_mode = WAL", [], |row| {
                row.get::<_, String>(0)
            })
            .map_err(storage)?;
        connection
            .execute_batch("PRAGMA synchronous = NORMAL;")
            .map_err(storage)?;
        connection.execute_batch(SCHEMA).map_err(storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn with<T>(&self, read: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "lexical index: connection lock poisoned".to_string())?;
        read(&connection)
    }

    pub(super) fn meta(&self) -> Result<SidecarMeta, String> {
        self.with(|connection| {
            let value = |key: &str| {
                connection
                    .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                        row.get::<_, String>(0)
                    })
                    .optional()
                    .map_err(storage)
            };
            Ok(SidecarMeta {
                version: value("index_version")?.unwrap_or_default(),
                profile: value("profile")?.unwrap_or_default(),
                position: value("position")?
                    .and_then(|position| position.parse().ok())
                    .unwrap_or(0),
                tail: value("tail")?.unwrap_or_default(),
            })
        })
    }

    pub(super) fn stats(&self, about: &str) -> Result<Option<AboutStats>, String> {
        self.with(|connection| {
            connection
                .query_row(
                    "SELECT stats FROM lex_stats WHERE about = ?1",
                    [about],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(storage)?
                .map(|stats| AboutStats::decode(&stats))
                .transpose()
        })
    }

    pub(super) fn node(&self, about: &str, node: &str) -> Result<Option<NodeState>, String> {
        self.with(|connection| {
            connection
                .query_row(
                    "SELECT state FROM lex_node WHERE about = ?1 AND node = ?2",
                    params![about, node],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
                .map_err(storage)?
                .map(|state| NodeState::decode(&state))
                .transpose()
        })
    }

    /// The signals of a relation the selection keeps, or none when it keeps
    /// no such relation.
    pub(super) fn relation(
        &self,
        about: &str,
        key: &RelationKey,
    ) -> Result<Option<LanguageSignals>, String> {
        self.with(|connection| {
            connection
                .query_row(
                    "SELECT signals FROM lex_relation \
                     WHERE about = ?1 AND source = ?2 AND target = ?3 AND type = ?4",
                    params![about, key.source, key.target, key.relation_type],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
                .map_err(storage)?
                .map(|signals| LanguageSignals::decode(&signals))
                .transpose()
        })
    }

    pub(super) fn row(&self, about: &str, doc: &str) -> Result<Option<LexicalRow>, String> {
        self.with(|connection| {
            connection
                .query_row(
                    "SELECT row FROM lex_fwd WHERE about = ?1 AND doc = ?2",
                    params![about, doc],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
                .map_err(storage)?
                .map(|row| LexicalRow::decode(&row))
                .transpose()
        })
    }

    /// Every row's fingerprint, by candidate id.
    pub(super) fn fingerprints(&self, about: &str) -> Result<BTreeMap<String, u64>, String> {
        self.with(|connection| {
            let mut statement = connection
                .prepare("SELECT doc, fingerprint FROM lex_fwd WHERE about = ?1")
                .map_err(storage)?;
            let rows = statement
                .query_map([about], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
                })
                .map_err(storage)?;
            rows.map(|row| row.map_err(storage)).collect()
        })
    }

    /// df of each term in the content and the direct field.
    pub(super) fn frequencies<'a>(
        &self,
        about: &str,
        terms: impl IntoIterator<Item = &'a String>,
    ) -> Result<BTreeMap<String, (u64, u64)>, String> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT content, direct FROM lex_key_df WHERE about = ?1 AND term = ?2",
                )
                .map_err(storage)?;
            terms
                .into_iter()
                .map(|term| {
                    let frequency = statement
                        .query_row(params![about, term], |row| {
                            Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64))
                        })
                        .optional()
                        .map_err(storage)?
                        .unwrap_or((0, 0));
                    Ok((term.clone(), frequency))
                })
                .collect()
        })
    }

    /// Every posting of a term, ascending by ordinal.
    pub(super) fn postings(&self, about: &str, term: &str) -> Result<Vec<Posting>, String> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT postings FROM lex_post WHERE about = ?1 AND term = ?2 ORDER BY block",
                )
                .map_err(storage)?;
            let blocks = statement
                .query_map(params![about, term], |row| row.get::<_, Vec<u8>>(0))
                .map_err(storage)?;
            let mut postings = Vec::new();
            for block in blocks {
                postings
                    .extend_from_slice(PostingBlock::decode(&block.map_err(storage)?)?.postings());
            }
            Ok(postings)
        })
    }

    /// The candidate id of each ordinal.
    pub(super) fn docs(&self, about: &str, ordinals: &[u64]) -> Result<Vec<String>, String> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached("SELECT doc FROM lex_fwd WHERE about = ?1 AND ordinal = ?2")
                .map_err(storage)?;
            ordinals
                .iter()
                .map(|ordinal| {
                    statement
                        .query_row(params![about, *ordinal as i64], |row| {
                            row.get::<_, String>(0)
                        })
                        .map_err(storage)
                })
                .collect()
        })
    }

    /// Drops everything held for one about, which the next ask of it builds
    /// again. The log position does not move: nothing else is affected.
    pub(super) fn forget(&self, about: &str) -> Result<(), String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "lexical index: connection lock poisoned".to_string())?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        for table in TABLES {
            tx.execute(&format!("DELETE FROM {table} WHERE about = ?1"), [about])
                .map_err(storage)?;
        }
        tx.commit().map_err(storage)
    }

    /// Writes what one maintenance pass decided, all or nothing, if the
    /// sidecar still stands where the pass read it (`expected`). `reset`
    /// empties it first, for a sidecar of another derivation or another log.
    /// Returns false, having written nothing, when another process moved it.
    pub(super) fn commit(
        &self,
        expected: &SidecarMeta,
        next: &SidecarMeta,
        reset: bool,
        changes: &[AboutChange],
    ) -> Result<bool, String> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "lexical index: connection lock poisoned".to_string())?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let held = {
            let value = |key: &str| {
                tx.query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                    row.get::<_, String>(0)
                })
                .optional()
                .map_err(storage)
            };
            (
                value("index_version")?.unwrap_or_default(),
                value("profile")?.unwrap_or_default(),
                value("position")?
                    .and_then(|position| position.parse::<u64>().ok())
                    .unwrap_or(0),
                value("tail")?.unwrap_or_default(),
            )
        };
        if held
            != (
                expected.version.clone(),
                expected.profile.clone(),
                expected.position,
                expected.tail.clone(),
            )
        {
            return Ok(false);
        }
        if reset {
            for table in TABLES {
                tx.execute(&format!("DELETE FROM {table}"), [])
                    .map_err(storage)?;
            }
        }
        for change in changes {
            write_change(&tx, change)?;
        }
        for (key, value) in [
            ("index_version", next.version.clone()),
            ("profile", next.profile.clone()),
            ("position", next.position.to_string()),
            ("tail", next.tail.clone()),
        ] {
            tx.execute(
                "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(true)
    }
}

fn write_change(tx: &Connection, change: &AboutChange) -> Result<(), String> {
    let about = change.about.as_str();
    let mut stats = if change.rebuilt {
        for table in TABLES {
            tx.execute(&format!("DELETE FROM {table} WHERE about = ?1"), [about])
                .map_err(storage)?;
        }
        AboutStats::default()
    } else {
        tx.query_row(
            "SELECT stats FROM lex_stats WHERE about = ?1",
            [about],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(storage)?
        .map(|stats| AboutStats::decode(&stats))
        .transpose()?
        .ok_or_else(|| format!("lexical index: about `{about}` is not built"))?
    };
    for (node, state) in &change.nodes {
        match state {
            Some(state) => tx.execute(
                "INSERT OR REPLACE INTO lex_node(about, node, state) VALUES (?1, ?2, ?3)",
                params![about, node, state.encode()],
            ),
            None => tx.execute(
                "DELETE FROM lex_node WHERE about = ?1 AND node = ?2",
                params![about, node],
            ),
        }
        .map_err(storage)?;
    }
    for (key, signals) in &change.relations {
        match signals {
            Some(signals) => tx.execute(
                "INSERT OR REPLACE INTO lex_relation(about, source, target, type, signals) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    about,
                    key.source,
                    key.target,
                    key.relation_type,
                    signals.encode()
                ],
            ),
            None => tx.execute(
                "DELETE FROM lex_relation \
                 WHERE about = ?1 AND source = ?2 AND target = ?3 AND type = ?4",
                params![about, key.source, key.target, key.relation_type],
            ),
        }
        .map_err(storage)?;
    }
    RowWriter::new(tx, about).apply(&mut stats, &change.rows)?;
    stats.signals = change.signals.clone();
    stats.summaries = change.summaries;
    stats.language = change.language.clone();
    tx.execute(
        "INSERT OR REPLACE INTO lex_stats(about, stats) VALUES (?1, ?2)",
        params![about, stats.encode()],
    )
    .map_err(storage)?;
    Ok(())
}

#[cfg(test)]
#[path = "sqlite_lexical_sidecar_tests.rs"]
mod tests;
