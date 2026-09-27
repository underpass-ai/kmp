use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use kmp_proto_mapping::v1beta1::{
    IndexedLifecycle, LanguageSignals, LexicalRow, Posting, PostingBlock, RelationClock,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::about_change::AboutChange;
use super::about_stats::AboutStats;
use super::kept_relation::KeptRelation;
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
CREATE TABLE IF NOT EXISTS lex_far(
    about TEXT NOT NULL, node TEXT NOT NULL, PRIMARY KEY (about, node)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS lex_clock(
    about TEXT NOT NULL, source TEXT NOT NULL, target TEXT NOT NULL, type TEXT NOT NULL,
    sequence INTEGER, secs INTEGER, nanos INTEGER, until TEXT,
    PRIMARY KEY (about, source, target, type)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS lex_clock_latest ON lex_clock(about, secs, nanos);
CREATE INDEX IF NOT EXISTS lex_clock_until ON lex_clock(about, until) WHERE until IS NOT NULL;
CREATE TABLE IF NOT EXISTS lex_vocab(
    about TEXT NOT NULL, word TEXT NOT NULL, docs INTEGER NOT NULL,
    PRIMARY KEY (about, word)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS lex_key_df(
    about TEXT NOT NULL, term TEXT NOT NULL, content INTEGER NOT NULL, direct INTEGER NOT NULL,
    aliased_content INTEGER NOT NULL, aliased_direct INTEGER NOT NULL,
    PRIMARY KEY (about, term)) WITHOUT ROWID;
";

const TABLES: [&str; 9] = [
    "lex_stats",
    "lex_node",
    "lex_relation",
    "lex_clock",
    "lex_vocab",
    "lex_far",
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
            .prepare_cached("PRAGMA journal_mode = WAL")
            .map_err(storage)?
            .query_row([], |row| row.get::<_, String>(0))
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
                    .prepare_cached("SELECT value FROM meta WHERE key = ?1")
                    .map_err(storage)?
                    .query_row([key], |row| row.get::<_, String>(0))
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
                .prepare_cached("SELECT stats FROM lex_stats WHERE about = ?1")
                .map_err(storage)?
                .query_row([about], |row| row.get::<_, String>(0))
                .optional()
                .map_err(storage)?
                .map(|stats| AboutStats::decode(&stats))
                .transpose()
        })
    }

    pub(super) fn node(&self, about: &str, node: &str) -> Result<Option<NodeState>, String> {
        self.with(|connection| {
            connection
                .prepare_cached("SELECT state FROM lex_node WHERE about = ?1 AND node = ?2")
                .map_err(storage)?
                .query_row(params![about, node], |row| row.get::<_, Vec<u8>>(0))
                .optional()
                .map_err(storage)?
                .map(|state| NodeState::decode(&state))
                .transpose()
        })
    }

    /// A relation the selection keeps, or none when it keeps no such
    /// relation.
    pub(super) fn relation(
        &self,
        about: &str,
        key: &RelationKey,
    ) -> Result<Option<KeptRelation>, String> {
        self.with(|connection| {
            let held = connection
                .prepare_cached(
                    "SELECT r.signals, c.sequence, c.secs, c.nanos, c.until FROM lex_relation r \
                     LEFT JOIN lex_clock c ON c.about = r.about AND c.source = r.source \
                     AND c.target = r.target AND c.type = r.type \
                     WHERE r.about = ?1 AND r.source = ?2 AND r.target = ?3 AND r.type = ?4",
                )
                .map_err(storage)?
                .query_row(
                    params![about, key.source, key.target, key.relation_type],
                    |row| {
                        Ok((
                            row.get::<_, Vec<u8>>(0)?,
                            row.get::<_, Option<i64>>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                            row.get::<_, Option<i64>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                        ))
                    },
                )
                .optional()
                .map_err(storage)?;
            let Some((signals, sequence, secs, nanos, until)) = held else {
                return Ok(None);
            };
            Ok(Some(KeptRelation {
                signals: LanguageSignals::decode(&signals)?,
                clock: RelationClock {
                    latest: secs.zip(nanos).map(|(secs, nanos)| (secs, nanos as i32)),
                    valid_until: until,
                    sequence: sequence.map(|sequence| sequence as u32),
                },
            }))
        })
    }

    /// The about's lifecycle as its kept relations declare it: the latest
    /// instant any carries, and every kept `contains_entry` edge that names
    /// a `valid_until` (DESIGN L6, P13).
    pub(super) fn lifecycle(&self, about: &str) -> Result<IndexedLifecycle, String> {
        self.with(|connection| {
            let frontier = connection
                .prepare_cached(
                    "SELECT secs, nanos FROM lex_clock WHERE about = ?1 AND secs IS NOT NULL \
                     ORDER BY secs DESC, nanos DESC LIMIT 1",
                )
                .map_err(storage)?
                .query_row([about], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)? as i32))
                })
                .optional()
                .map_err(storage)?;
            let mut statement = connection
                .prepare(
                    "SELECT sequence, source, target, until FROM lex_clock \
                     WHERE about = ?1 AND until IS NOT NULL AND type = 'contains_entry'",
                )
                .map_err(storage)?;
            let expiring = statement
                .query_map([about], |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?
                            .map(|sequence| sequence as u32),
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(storage)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(storage)?;
            Ok(IndexedLifecycle::new(frontier, expiring))
        })
    }

    /// Whether a node lies one hop past the ask's depth.
    pub(super) fn is_far(&self, about: &str, node: &str) -> Result<bool, String> {
        self.with(|connection| {
            connection
                .prepare_cached("SELECT 1 FROM lex_far WHERE about = ?1 AND node = ?2")
                .map_err(storage)?
                .query_row(params![about, node], |_| Ok(()))
                .optional()
                .map(|found| found.is_some())
                .map_err(storage)
        })
    }

    pub(super) fn row(&self, about: &str, doc: &str) -> Result<Option<LexicalRow>, String> {
        self.with(|connection| {
            connection
                .prepare_cached("SELECT row FROM lex_fwd WHERE about = ?1 AND doc = ?2")
                .map_err(storage)?
                .query_row(params![about, doc], |row| row.get::<_, Vec<u8>>(0))
                .optional()
                .map_err(storage)?
                .map(|row| LexicalRow::decode(&row))
                .transpose()
        })
    }

    /// Every row's fingerprint under a reading, by candidate id.
    pub(super) fn fingerprints(
        &self,
        about: &str,
        aliased: bool,
    ) -> Result<BTreeMap<String, u64>, String> {
        if aliased {
            // The aliased fingerprint is kept beside the row.
            return self.with(|connection| {
                let mut statement = connection
                    .prepare("SELECT doc, fingerprint FROM lex_fwd WHERE about = ?1")
                    .map_err(storage)?;
                let rows = statement
                    .query_map([about], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
                    })
                    .map_err(storage)?;
                rows.map(|row| row.map_err(storage)).collect()
            });
        }
        self.with(|connection| {
            let mut statement = connection
                .prepare("SELECT doc, row FROM lex_fwd WHERE about = ?1")
                .map_err(storage)?;
            let rows = statement
                .query_map([about], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
                })
                .map_err(storage)?;
            rows.map(|row| {
                let (doc, bytes) = row.map_err(storage)?;
                Ok((doc, LexicalRow::decode(&bytes)?.fingerprint(false)))
            })
            .collect()
        })
    }

    /// df of each term in the content and the direct field, under a reading.
    pub(super) fn frequencies<'a>(
        &self,
        about: &str,
        terms: impl IntoIterator<Item = &'a String>,
        aliased: bool,
    ) -> Result<BTreeMap<String, (u64, u64)>, String> {
        let sql = if aliased {
            "SELECT aliased_content, aliased_direct FROM lex_key_df WHERE about = ?1 AND term = ?2"
        } else {
            "SELECT content, direct FROM lex_key_df WHERE about = ?1 AND term = ?2"
        };
        self.with(|connection| {
            let mut statement = connection.prepare_cached(sql).map_err(storage)?;
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

    /// Every word the about's candidates' texts carry, ascending: the
    /// vocabulary the lexical bridge reads (DESIGN L6, P13).
    pub(super) fn vocabulary(&self, about: &str) -> Result<Vec<String>, String> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached("SELECT word FROM lex_vocab WHERE about = ?1 ORDER BY word")
                .map_err(storage)?;
            let words = statement
                .query_map([about], |row| row.get::<_, String>(0))
                .map_err(storage)?;
            words.map(|word| word.map_err(storage)).collect()
        })
    }

    /// df of each term held in the about, under both readings: content and
    /// direct, plain then aliased. A term no candidate holds is absent.
    pub(super) fn document_frequencies(
        &self,
        about: &str,
        terms: &std::collections::BTreeSet<String>,
    ) -> Result<BTreeMap<String, [u64; 4]>, String> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT content, direct, aliased_content, aliased_direct FROM lex_key_df \
                     WHERE about = ?1 AND term = ?2",
                )
                .map_err(storage)?;
            let mut held = BTreeMap::new();
            for term in terms {
                let frequency = statement
                    .query_row(params![about, term], |row| {
                        Ok([
                            row.get::<_, i64>(0)? as u64,
                            row.get::<_, i64>(1)? as u64,
                            row.get::<_, i64>(2)? as u64,
                            row.get::<_, i64>(3)? as u64,
                        ])
                    })
                    .optional()
                    .map_err(storage)?;
                if let Some(frequency) = frequency {
                    held.insert(term.clone(), frequency);
                }
            }
            Ok(held)
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
        if reset || changes.iter().any(|change| change.rebuilt) {
            // A build writes the whole about in one transaction; hand the
            // log's pages back rather than keep a WAL the size of the index.
            // Best effort: a reader holding the log only delays it.
            let _ = connection
                .prepare_cached("PRAGMA wal_checkpoint(TRUNCATE)")
                .map_err(storage)?
                .query_row([], |_| Ok(()));
        }
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
    for (key, kept) in &change.relations {
        let identity = params![about, key.source, key.target, key.relation_type];
        match kept {
            Some(kept) => {
                tx.execute(
                    "INSERT OR REPLACE INTO lex_relation(about, source, target, type, signals) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        about,
                        key.source,
                        key.target,
                        key.relation_type,
                        kept.signals.encode()
                    ],
                )
                .map_err(storage)?;
                let clock = &kept.clock;
                tx.execute(
                    "INSERT OR REPLACE INTO lex_clock\
                     (about, source, target, type, sequence, secs, nanos, until) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        about,
                        key.source,
                        key.target,
                        key.relation_type,
                        clock.sequence.map(i64::from),
                        clock.latest.map(|(secs, _)| secs),
                        clock.latest.map(|(_, nanos)| i64::from(nanos)),
                        clock.valid_until,
                    ],
                )
                .map_err(storage)?;
            }
            None => {
                for table in ["lex_relation", "lex_clock"] {
                    tx.execute(
                        &format!(
                            "DELETE FROM {table} \
                             WHERE about = ?1 AND source = ?2 AND target = ?3 AND type = ?4"
                        ),
                        identity,
                    )
                    .map_err(storage)?;
                }
            }
        }
    }
    for (node, far) in &change.far {
        let moved = if *far {
            tx.execute(
                "INSERT OR IGNORE INTO lex_far(about, node) VALUES (?1, ?2)",
                params![about, node],
            )
        } else {
            tx.execute(
                "DELETE FROM lex_far WHERE about = ?1 AND node = ?2",
                params![about, node],
            )
        }
        .map_err(storage)? as u64;
        stats.far = if *far {
            stats.far + moved
        } else {
            stats.far.saturating_sub(moved)
        };
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
