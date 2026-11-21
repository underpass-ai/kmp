use std::collections::{BTreeMap, BTreeSet};

use kmp_proto_mapping::v1beta1::{LexicalReach, LexicalRow};
use rusqlite::{OptionalExtension, params};

use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use super::storage;
use crate::serving::lexical_totals::LexicalTotals;
use crate::serving::ports::lexical_candidates::LexicalCandidates;

impl LexicalCandidates for SqliteLexicalSidecar {
    fn totals(&self, about: &str, aliased: bool) -> Result<Option<LexicalTotals>, String> {
        Ok(self.stats(about)?.map(|stats| LexicalTotals {
            documents: stats.documents,
            content_length: stats.content_length(aliased),
            direct_length: stats.direct_length(aliased),
            language: stats.language,
        }))
    }

    fn frequencies(
        &self,
        about: &str,
        terms: &[String],
        aliased: bool,
    ) -> Result<BTreeMap<String, (u64, u64)>, String> {
        SqliteLexicalSidecar::frequencies(self, about, terms, aliased)
    }

    fn candidates(
        &self,
        about: &str,
        terms: &[String],
    ) -> Result<BTreeMap<String, LexicalRow>, String> {
        let mut ordinals = BTreeSet::new();
        for term in terms {
            ordinals.extend(self.postings(about, term)?.iter().map(|p| p.ordinal));
        }
        // One statement per candidate: its id and its row, by ordinal.
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached("SELECT doc, row FROM lex_fwd WHERE about = ?1 AND ordinal = ?2")
                .map_err(storage)?;
            let mut candidates = BTreeMap::new();
            for ordinal in ordinals {
                let (doc, row) = statement
                    .query_row(params![about, ordinal as i64], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
                    })
                    .map_err(storage)?;
                candidates.insert(doc, LexicalRow::decode(&row)?);
            }
            Ok(candidates)
        })
    }
}

/// What a candidate's graph lends it (P14), read beside its row.
impl SqliteLexicalSidecar {
    /// The reach of each of `docs` that has one ([`LexicalReach`]).
    pub(super) fn reaches<'d>(
        &self,
        about: &str,
        docs: impl IntoIterator<Item = &'d String>,
    ) -> Result<BTreeMap<String, LexicalReach>, String> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached("SELECT reach FROM lex_reach WHERE about = ?1 AND doc = ?2")
                .map_err(storage)?;
            let mut held = BTreeMap::new();
            for doc in docs {
                let reach = statement
                    .query_row(params![about, doc], |row| row.get::<_, Vec<u8>>(0))
                    .optional()
                    .map_err(storage)?;
                if let Some(reach) = reach {
                    held.insert(doc.clone(), LexicalReach::decode(&reach)?);
                }
            }
            Ok(held)
        })
    }
}
