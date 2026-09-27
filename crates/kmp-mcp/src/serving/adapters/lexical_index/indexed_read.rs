use kmp_application::GetContextResult;
use kmp_proto_mapping::v1beta1::IndexedAsk;

/// An ask's read answered from the lexical index (DESIGN L6, P13): the
/// part of the about the postings reached, the whole about as the index
/// describes it, and how many candidates the postings reached.
pub(crate) struct IndexedRead {
    pub(crate) result: GetContextResult,
    pub(crate) indexed: IndexedAsk,
    pub(crate) candidates: usize,
    /// How many candidates the whole about holds.
    pub(crate) documents: u64,
    /// Microseconds spent choosing the candidates, then reading them and
    /// their neighbourhood from the store.
    pub(crate) plan_us: u64,
    pub(crate) parts_us: u64,
}
