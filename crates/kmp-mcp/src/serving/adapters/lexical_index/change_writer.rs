use rusqlite::{Connection, OptionalExtension, params};

use super::about_change::AboutChange;
use super::about_stats::AboutStats;
use super::row_writer::RowWriter;
use super::sqlite_lexical_sidecar::TABLES;
use super::storage;

/// Writes what one maintenance pass decided for one about inside the
/// sidecar's transaction: the node states, the kept relations and their
/// clocks, the far nodes, the rows and the about's totals.
pub(super) fn write_change(tx: &Connection, change: &AboutChange) -> Result<(), String> {
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
