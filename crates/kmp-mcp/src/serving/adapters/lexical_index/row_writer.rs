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
    /// df deltas: content and direct, plain then aliased.
    frequencies: BTreeMap<String, [i64; 4]>,
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
                                row.fingerprint(true) as i64,
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
        self.account(stats, ordinal, row, -1);
        Ok(())
    }

    fn contribute(&mut self, stats: &mut AboutStats, ordinal: u64, row: &LexicalRow) {
        stats.documents += 1;
        self.account(stats, ordinal, row, 1);
    }

    /// Adds (`sign` 1) or takes away (-1) a row's lengths, fingerprints, df
    /// and postings.
    fn account(&mut self, stats: &mut AboutStats, ordinal: u64, row: &LexicalRow, sign: i64) {
        let [text, summary, extra, alias_content, alias_extra] = row.lengths();
        stats.text_length += sign * text;
        stats.summary_length += sign * summary;
        stats.extra_length += sign * extra;
        stats.alias_content_length += sign * alias_content;
        stats.alias_extra_length += sign * alias_extra;
        let (plain, aliased) = (row.fingerprint(false), row.fingerprint(true));
        if sign > 0 {
            stats.rows_digest = stats.rows_digest.wrapping_add(plain);
            stats.aliased_digest = stats.aliased_digest.wrapping_add(aliased);
        } else {
            stats.rows_digest = stats.rows_digest.wrapping_sub(plain);
            stats.aliased_digest = stats.aliased_digest.wrapping_sub(aliased);
        }
        for term in row.terms().iter().filter(|term| term.is_held()) {
            let frequency = self.frequencies.entry(term.term.clone()).or_default();
            for (slot, reading) in [(0, false), (2, true)] {
                frequency[slot] += sign * i64::from(term.content(reading) > 0);
                frequency[slot + 1] += sign * i64::from(term.direct(reading) > 0);
            }
            let posting = (sign > 0).then(|| Posting {
                ordinal,
                content: term.content(false).max(0) as u64,
                direct: term.direct(false).max(0) as u64,
                aliased_content: term.content(true).max(0) as u64,
                aliased_direct: term.direct(true).max(0) as u64,
            });
            self.postings
                .entry(term.term.clone())
                .or_default()
                .push((ordinal, posting));
        }
    }

    fn flush(&mut self) -> Result<(), String> {
        for (term, delta) in std::mem::take(&mut self.frequencies) {
            if delta == [0; 4] {
                continue;
            }
            let held = self
                .tx
                .query_row(
                    "SELECT content, direct, aliased_content, aliased_direct FROM lex_key_df \
                     WHERE about = ?1 AND term = ?2",
                    params![self.about, term],
                    |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?]),
                )
                .optional()
                .map_err(storage)?
                .unwrap_or([0i64; 4]);
            let next = [
                held[0] + delta[0],
                held[1] + delta[1],
                held[2] + delta[2],
                held[3] + delta[3],
            ];
            if next.iter().any(|count| *count < 0) {
                return Err(format!("lexical index: df of `{term}` below zero"));
            }
            if next == [0; 4] {
                self.tx
                    .execute(
                        "DELETE FROM lex_key_df WHERE about = ?1 AND term = ?2",
                        params![self.about, term],
                    )
                    .map_err(storage)?;
            } else {
                self.tx
                    .execute(
                        "INSERT OR REPLACE INTO lex_key_df\
                         (about, term, content, direct, aliased_content, aliased_direct) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![self.about, term, next[0], next[1], next[2], next[3]],
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
