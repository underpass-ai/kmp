use std::collections::BTreeMap;

/// One reading of an about's collection statistics, as the lexical sidecar
/// holds them (DESIGN L6, P13): N, Σ length of the content and the direct
/// field, and df in each field of every term an ask may weigh or meet in a
/// candidate it reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexedFieldStats {
    pub documents: u64,
    pub content_length: i64,
    pub direct_length: i64,
    pub content_df: BTreeMap<String, u64>,
    pub direct_df: BTreeMap<String, u64>,
}
