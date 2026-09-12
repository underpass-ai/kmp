/// Why a body expansion was refused before any body was read.
///
/// Both cases are decided from the manifest alone, and in both the response
/// carries no canonical text and no card text. Joining chunks of two different
/// selections by ref would produce a proof nobody selected — old graph, new
/// text — and quietly dropping a ref nobody selected would answer a question
/// that was never asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceExpansionRefusal {
    /// The caller declared a manifest this snapshot no longer has. The store
    /// keeps only current bodies, so the answer is a fresh read, never a
    /// reconstructed old one.
    SelectionChanged {
        /// The manifest the caller declared.
        expected: String,
        /// The manifest this snapshot actually has.
        actual: String,
    },
    /// The caller named refs that are not in the selected proof table. The
    /// whole batch is refused rather than widening the selection to fit them
    /// or silently delivering only the ones that happened to be inside.
    UnknownRefs(Vec<String>),
}

impl TraceExpansionRefusal {
    pub const fn code(&self) -> &'static str {
        match self {
            TraceExpansionRefusal::SelectionChanged { .. } => "read_selection_changed",
            TraceExpansionRefusal::UnknownRefs(_) => "unknown_expansion_refs",
        }
    }
}
