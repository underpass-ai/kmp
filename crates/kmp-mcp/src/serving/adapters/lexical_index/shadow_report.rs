/// How the sidecar compared with what one ask's ranker measured (DESIGN L6,
/// shadow mode). Every counter is a difference; a sound sidecar reports
/// zero everywhere.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ShadowReport {
    /// Whether the ask read what the sidecar indexes: one about, its default
    /// depth, no dimensions, the frontier, the same reading, and a store that
    /// did not move while it asked. When false the counters stay zero and
    /// `reason` says why.
    pub(crate) comparable: bool,
    pub(crate) reason: &'static str,
    pub(crate) documents: u64,
    /// N, Σlen content, Σlen direct and the language that differ.
    pub(crate) stats_differences: u64,
    /// Weighted terms whose df differs in either field.
    pub(crate) df_differences: u64,
    /// Candidates whose row differs, is missing or is extra.
    pub(crate) row_differences: u64,
    /// Candidates that could score above zero and no posting of a weighted
    /// term reaches: the one thing a candidate generator must never do.
    pub(crate) missing_candidates: u64,
    /// An ask narrowed by dimensions reads a subset of the about, whose
    /// statistics and language are its own: not a difference of the index,
    /// but what P13 must know before serving such an ask from it. Candidates
    /// whose row differs from the about's, or that the about lacks, and
    /// whether the subset reads in another language.
    pub(crate) selection_rows: u64,
    pub(crate) selection_language: bool,
    pub(crate) elapsed_us: u64,
}

impl ShadowReport {
    pub(crate) fn not_comparable(reason: &'static str) -> Self {
        Self {
            comparable: false,
            reason,
            ..Self::default()
        }
    }

    /// Every difference, summed.
    pub(crate) fn differences(&self) -> u64 {
        self.stats_differences
            + self.df_differences
            + self.row_differences
            + self.missing_candidates
    }

    /// One line in the server log, which the mixed-version bench reads.
    pub(crate) fn emit(&self, about: &str) {
        tracing::info!(
            target: "kmp_mcp::lexical_index",
            event = "kmp_lexical_shadow",
            about,
            comparable = self.comparable,
            reason = self.reason,
            documents = self.documents,
            differences = self.differences(),
            stats_differences = self.stats_differences,
            df_differences = self.df_differences,
            row_differences = self.row_differences,
            missing_candidates = self.missing_candidates,
            selection_rows = self.selection_rows,
            selection_language = self.selection_language,
            elapsed_us = self.elapsed_us,
            "lexical index compared in shadow"
        );
    }
}
