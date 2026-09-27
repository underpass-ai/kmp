use std::collections::BTreeMap;

use kmp_proto_mapping::v1beta1::{LexicalRow, Posting};
use rusqlite::{Connection, OptionalExtension, params};

use super::about_stats::AboutStats;
use super::term_postings::TermPostings;

use super::storage;

/// Rows between two flushes of the postings and frequencies they gathered,
/// so building a large about does not hold every posting at once.
const FLUSH_EVERY: usize = 8192;

/// Applies candidate rows to one about inside a sidecar transaction: the
/// forward rows (`lex_fwd`), the posting blocks (`lex_post`), the document
/// frequencies (`lex_key_df`) and the about's totals. A row equal to the one
/// held is not touched, so the same rows applied twice change nothing.
pub(super) struct RowWriter<'t> {
    tx: &'t Connection,
    about: &'t str,
    postings: BTreeMap<String, Vec<(u64, Option<Posting>)>>,
    frequencies: BTreeMap<String, (i64, i64)>,
}

impl<'t> RowWriter<'t> {
    pub(super) fn new(tx: &'t Connection, about: &'t str) -> Self {
        Self {
            tx,
            about,
            postings: BTreeMap::new(),
            frequencies: BTreeMap::new(),
        }
    }

    /// Replaces, adds or (with `None`) removes each row, updating `stats`.
    /// Returns how many rows actually changed.
    pub(super) fn apply(
        mut self,
        stats: &mut AboutStats,
        rows: &[(String, Option<LexicalRow>)],
    ) -> Result<usize, String> {
        let mut changed = 0;
        for (index, (doc, new)) in rows.iter().enumerate() {
            if index > 0 && index % FLUSH_EVERY == 0 {
                self.flush()?;
            }
            let held = self.held(doc)?;
            if held.as_ref().map(|(_, row)| row) == new.as_ref() {
                continue;
            }
            changed += 1;
            let ordinal = match &held {
                Some((ordinal, _)) => *ordinal,
                None => {
                    let ordinal = stats.next_ordinal;
                    stats.next_ordinal += 1;
                    ordinal
                }
            };
            if let Some((_, old)) = &held {
                self.withdraw(stats, ordinal, old)?;
            }
            match new {
                Some(row) => {
                    self.contribute(stats, ordinal, row);
                    self.tx
                        .execute(
                            "INSERT OR REPLACE INTO lex_fwd(about, doc, ordinal, fingerprint, row) \
                             VALUES (?1, ?2, ?3, ?4, ?5)",
                            params![
                                self.about,
                                doc,
                                ordinal as i64,
                                row.fingerprint() as i64,
                                row.encode()
                            ],
                        )
                        .map_err(storage)?;
                }
                None => {
                    self.tx
                        .execute(
                            "DELETE FROM lex_fwd WHERE about = ?1 AND doc = ?2",
                            params![self.about, doc],
                        )
                        .map_err(storage)?;
                }
            }
        }
        self.flush()?;
        Ok(changed)
    }

    fn held(&self, doc: &str) -> Result<Option<(u64, LexicalRow)>, String> {
        let held = self
            .tx
            .query_row(
                "SELECT ordinal, row FROM lex_fwd WHERE about = ?1 AND doc = ?2",
                params![self.about, doc],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(storage)?;
        held.map(|(ordinal, bytes)| Ok((ordinal as u64, LexicalRow::decode(&bytes)?)))
            .transpose()
    }

    fn withdraw(
        &mut self,
        stats: &mut AboutStats,
        ordinal: u64,
        row: &LexicalRow,
    ) -> Result<(), String> {
        stats.documents = stats
            .documents
            .checked_sub(1)
            .ok_or("lexical index: document count below zero")?;
        let (text, summary, extra) = row.lengths();
        stats.text_length -= text;
        stats.summary_length -= summary;
        stats.extra_length -= extra;
        stats.rows_digest = stats.rows_digest.wrapping_sub(row.fingerprint());
        for term in row.terms().iter().filter(|term| term.is_searchable()) {
            let frequency = self.frequencies.entry(term.term.clone()).or_default();
            frequency.0 -= i64::from(term.content() > 0);
            frequency.1 -= i64::from(term.direct() > 0);
            self.postings
                .entry(term.term.clone())
                .or_default()
                .push((ordinal, None));
        }
        Ok(())
    }

    fn contribute(&mut self, stats: &mut AboutStats, ordinal: u64, row: &LexicalRow) {
        stats.documents += 1;
        let (text, summary, extra) = row.lengths();
        stats.text_length += text;
        stats.summary_length += summary;
        stats.extra_length += extra;
        stats.rows_digest = stats.rows_digest.wrapping_add(row.fingerprint());
        for term in row.terms().iter().filter(|term| term.is_searchable()) {
            let frequency = self.frequencies.entry(term.term.clone()).or_default();
            frequency.0 += i64::from(term.content() > 0);
            frequency.1 += i64::from(term.direct() > 0);
            self.postings.entry(term.term.clone()).or_default().push((
                ordinal,
                Some(Posting {
                    ordinal,
                    content: term.content().max(0) as u64,
                    direct: term.direct().max(0) as u64,
                }),
            ));
        }
    }

    fn flush(&mut self) -> Result<(), String> {
        for (term, (content, direct)) in std::mem::take(&mut self.frequencies) {
            if content == 0 && direct == 0 {
                continue;
            }
            let held = self
                .tx
                .query_row(
                    "SELECT content, direct FROM lex_key_df WHERE about = ?1 AND term = ?2",
                    params![self.about, term],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()
                .map_err(storage)?
                .unwrap_or((0, 0));
            let next = (held.0 + content, held.1 + direct);
            if next.0 < 0 || next.1 < 0 {
                return Err(format!("lexical index: df of `{term}` below zero"));
            }
            if next == (0, 0) {
                self.tx
                    .execute(
                        "DELETE FROM lex_key_df WHERE about = ?1 AND term = ?2",
                        params![self.about, term],
                    )
                    .map_err(storage)?;
            } else {
                self.tx
                    .execute(
                        "INSERT OR REPLACE INTO lex_key_df(about, term, content, direct) \
                         VALUES (?1, ?2, ?3, ?4)",
                        params![self.about, term, next.0, next.1],
                    )
                    .map_err(storage)?;
            }
        }
        for (term, operations) in std::mem::take(&mut self.postings) {
            TermPostings::new(self.tx, self.about, &term).apply(&operations)?;
        }
        Ok(())
    }
}
