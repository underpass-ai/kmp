use std::collections::BTreeMap;

use kmp_proto_mapping::v1beta1::LexicalRow;

use crate::serving::lexical_totals::LexicalTotals;

/// What the lexical index answers for one about (DESIGN L6), the interface
/// P13 generates candidates through: the collection statistics BM25 needs,
/// df of the question's terms, and the candidates the postings of those terms
/// reach, with the rows to score them. Everything answers the about as the
/// index last followed it; nothing here reads the store. Calls block briefly
/// on local storage; callers run them off the async runtime.
pub(crate) trait LexicalCandidates: Send + Sync {
    /// N, Σlen per field and the language, or none when the about is not
    /// built yet.
    fn totals(&self, about: &str) -> Result<Option<LexicalTotals>, String>;

    /// df of each term in the content and the direct field.
    fn frequencies(
        &self,
        about: &str,
        terms: &[String],
    ) -> Result<BTreeMap<String, (u64, u64)>, String>;

    /// Every candidate a posting of any of `terms` reaches, by candidate id,
    /// with its row. Outside this set no candidate can score above zero.
    fn candidates(
        &self,
        about: &str,
        terms: &[String],
    ) -> Result<BTreeMap<String, LexicalRow>, String>;
}
