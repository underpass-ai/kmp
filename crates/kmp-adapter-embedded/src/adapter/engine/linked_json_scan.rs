use super::Table;

/// Bounded ordered join of a root-key set, typed links and JSON records.
/// Field paths and the relation kind are values, never SQL fragments.
pub(crate) struct LinkedJsonScan<'a> {
    pub roots: Table,
    pub links: Table,
    pub records: Table,
    pub relation: &'a str,
    pub fields: &'a [&'a str],
    pub after: Option<(&'a str, &'a str)>,
    pub limit: u32,
}
