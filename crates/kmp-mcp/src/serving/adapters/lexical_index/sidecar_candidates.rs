use std::collections::{BTreeMap, BTreeSet};

use kmp_proto_mapping::v1beta1::LexicalRow;

use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use crate::serving::lexical_totals::LexicalTotals;
use crate::serving::ports::lexical_candidates::LexicalCandidates;

impl LexicalCandidates for SqliteLexicalSidecar {
    fn totals(&self, about: &str) -> Result<Option<LexicalTotals>, String> {
        Ok(self.stats(about)?.map(|stats| LexicalTotals {
            documents: stats.documents,
            content_length: stats.content_length(),
            direct_length: stats.direct_length(),
            language: stats.language,
        }))
    }

    fn frequencies(
        &self,
        about: &str,
        terms: &[String],
    ) -> Result<BTreeMap<String, (u64, u64)>, String> {
        SqliteLexicalSidecar::frequencies(self, about, terms)
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
        let ordinals = ordinals.into_iter().collect::<Vec<_>>();
        let mut candidates = BTreeMap::new();
        for doc in self.docs(about, &ordinals)? {
            let row = self
                .row(about, &doc)?
                .ok_or_else(|| format!("lexical index: `{doc}` has no row"))?;
            candidates.insert(doc, row);
        }
        Ok(candidates)
    }
}
