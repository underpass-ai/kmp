use std::collections::BTreeMap;

use kmp_proto_mapping::v1beta1::{Posting, PostingBlock};
use rusqlite::{Connection, OptionalExtension, params};

use super::storage;

/// The posting blocks of one term, changed in place: each ordinal goes to
/// the block whose first ordinal is the greatest at or below it, a block that
/// grows past its capacity splits, an emptied one goes, and every block is
/// keyed by its first ordinal again when written back.
pub(super) struct TermPostings<'t> {
    tx: &'t Connection,
    about: &'t str,
    term: &'t str,
    blocks: BTreeMap<u64, PostingBlock>,
}

impl<'t> TermPostings<'t> {
    pub(super) fn new(tx: &'t Connection, about: &'t str, term: &'t str) -> Self {
        Self {
            tx,
            about,
            term,
            blocks: BTreeMap::new(),
        }
    }

    pub(super) fn apply(mut self, operations: &[(u64, Option<Posting>)]) -> Result<(), String> {
        let last = self.block_key(
            "SELECT block FROM lex_post WHERE about = ?1 AND term = ?2 ORDER BY block DESC LIMIT 1",
            None,
        )?;
        for (ordinal, operation) in operations {
            // The tail block: the last one held, or the last one gathered
            // here. Appends, the usual case, need no lookup at all.
            let tail = last.into_iter().chain(self.blocks.keys().copied()).max();
            let key = match tail {
                Some(tail) if *ordinal >= tail => Some(tail),
                _ => self
                    .block_key(
                        "SELECT block FROM lex_post WHERE about = ?1 AND term = ?2 AND block <= ?3 \
                         ORDER BY block DESC LIMIT 1",
                        Some(*ordinal),
                    )?
                    .or(self.block_key(
                        "SELECT block FROM lex_post WHERE about = ?1 AND term = ?2 \
                         ORDER BY block ASC LIMIT 1",
                        None,
                    )?),
            };
            let key = key.unwrap_or(*ordinal);
            if !self.blocks.contains_key(&key) {
                let block = self.load(key)?;
                self.blocks.insert(key, block);
            }
            let block = self.blocks.get_mut(&key).expect("loaded above");
            match operation {
                Some(posting) => block.upsert(*posting),
                None => block.remove(*ordinal),
            }
        }
        for (key, block) in std::mem::take(&mut self.blocks) {
            self.tx
                .execute(
                    "DELETE FROM lex_post WHERE about = ?1 AND term = ?2 AND block = ?3",
                    params![self.about, self.term, key as i64],
                )
                .map_err(storage)?;
            for part in block.split() {
                if let Some(first) = part.first_ordinal() {
                    self.tx
                        .execute(
                            "INSERT OR REPLACE INTO lex_post(about, term, block, postings) \
                             VALUES (?1, ?2, ?3, ?4)",
                            params![self.about, self.term, first as i64, part.encode()],
                        )
                        .map_err(storage)?;
                }
            }
        }
        Ok(())
    }

    fn block_key(&self, sql: &str, bound: Option<u64>) -> Result<Option<u64>, String> {
        let key = match bound {
            Some(bound) => self
                .tx
                .query_row(sql, params![self.about, self.term, bound as i64], |row| {
                    row.get::<_, i64>(0)
                })
                .optional(),
            None => self
                .tx
                .query_row(sql, params![self.about, self.term], |row| {
                    row.get::<_, i64>(0)
                })
                .optional(),
        };
        Ok(key.map_err(storage)?.map(|key| key as u64))
    }

    fn load(&self, key: u64) -> Result<PostingBlock, String> {
        let bytes = self
            .tx
            .query_row(
                "SELECT postings FROM lex_post WHERE about = ?1 AND term = ?2 AND block = ?3",
                params![self.about, self.term, key as i64],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(storage)?;
        match bytes {
            Some(bytes) => PostingBlock::decode(&bytes),
            None => Ok(PostingBlock::default()),
        }
    }
}
