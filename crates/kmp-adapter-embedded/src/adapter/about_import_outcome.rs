use serde::Serialize;

/// What `import --from --about` did to one about of the destination.
///
/// An about is the set of event streams rooted at it, one per role. The
/// outcome summarises them all: nothing of it was here, all of it already
/// was, or the destination held an exact prefix that was carried forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AboutImportOutcome {
    /// The destination held no event of this about; every one was replayed.
    Imported,
    /// The destination already held exactly the source's events.
    Unchanged,
    /// The destination held an exact prefix; only the missing tail was
    /// appended, on the same revisions the source recorded.
    Extended,
}

impl AboutImportOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Imported => "imported",
            Self::Unchanged => "unchanged",
            Self::Extended => "extended",
        }
    }
}
