//! What the search box holds.

/// The search text on a view. The loom's browser half interprets it
/// (`kind:` and `dim:` tokens included); the aggregate only carries it so
/// both faces read the same filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchQuery(String);

impl SearchQuery {
    /// A query exactly as typed or intended.
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// The query text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
