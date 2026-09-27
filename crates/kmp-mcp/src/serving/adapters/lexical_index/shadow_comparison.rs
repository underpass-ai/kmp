use kmp_proto_mapping::v1beta1::LexicalObservation;

use super::shadow_report::ShadowReport;
use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use crate::serving::ports::lexical_candidates::LexicalCandidates;

/// Compares the sidecar's view of one about with what an ask's ranker
/// measured over the same candidates: the integers BM25 reads (N, Σlen per
/// field, df of every weighted term, tf and length of every candidate, by
/// fingerprint, its judged expansions included) and whether the postings of the weighted terms reach every
/// candidate that could score. The ranker's answer is never touched.
pub(super) struct ShadowComparison<'s> {
    sidecar: &'s SqliteLexicalSidecar,
}

impl<'s> ShadowComparison<'s> {
    pub(super) fn new(sidecar: &'s SqliteLexicalSidecar) -> Self {
        Self { sidecar }
    }

    /// Whether a deeper ask reaches nodes the sidecar does not index: only
    /// when something lies one hop past the default depth.
    pub(super) fn reads_past(&self, about: &str, deeper: bool) -> Result<bool, String> {
        Ok(deeper
            && self
                .sidecar
                .stats(about)?
                .is_some_and(|stats| stats.far > 0))
    }

    pub(super) fn compare(
        &self,
        about: &str,
        observation: &LexicalObservation,
    ) -> Result<ShadowReport, String> {
        let started = std::time::Instant::now();
        let aliased = observation.aliased();
        let (Some(totals), Some(stats)) = (
            self.sidecar.totals(about, aliased)?,
            self.sidecar.stats(about)?,
        ) else {
            return Ok(ShadowReport::not_comparable("about not built"));
        };
        let mut report = ShadowReport {
            comparable: true,
            reason: "compared",
            documents: observation.documents(),
            ..ShadowReport::default()
        };
        let (content, direct) = observation.lengths();
        report.stats_differences = [
            totals.documents != observation.documents(),
            totals.content_length != content,
            totals.direct_length != direct,
            totals.language.as_deref() != observation.language(),
            (stats.expanded, stats.expansion_length) != observation.expansions(),
        ]
        .into_iter()
        .map(u64::from)
        .sum();
        // What P13 will read: df of the weighted terms, and the candidates
        // their postings reach with the rows to score them.
        let weighted = observation
            .frequencies()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let frequencies = LexicalCandidates::frequencies(self.sidecar, about, &weighted, aliased)?;
        report.df_differences = observation
            .frequencies()
            .iter()
            .filter(|(term, frequency)| frequencies.get(*term) != Some(*frequency))
            .count() as u64;
        let candidates = if observation.scored().is_empty() {
            Default::default()
        } else {
            self.sidecar.candidates(about, &weighted)?
        };
        let seen = observation.fingerprints();
        for doc in observation.scored() {
            match candidates.get(doc) {
                None => report.missing_candidates += 1,
                Some(row) if seen.get(doc) != Some(&row.fingerprint(aliased)) => {
                    report.row_differences += 1;
                }
                Some(_) => {}
            }
        }
        // Every other candidate, through the digest of all their rows.
        if stats.digest(aliased) != observation.rows_digest() && report.row_differences == 0 {
            let held = self.sidecar.fingerprints(about, aliased)?;
            report.row_differences = held
                .iter()
                .filter(|(doc, fingerprint)| seen.get(*doc) != Some(*fingerprint))
                .count() as u64
                + seen.keys().filter(|doc| !held.contains_key(*doc)).count() as u64;
            // A collision of the digest alone still counts once.
            report.row_differences = report.row_differences.max(1);
        }
        report.elapsed_us = started.elapsed().as_micros() as u64;
        Ok(report)
    }

    /// An ask narrowed by dimensions: its candidates are a subset of the
    /// about's, measured in the subset's own statistics and language. What
    /// can be compared is each candidate's row and the language; the counts
    /// are reported apart and are not differences of the index.
    pub(super) fn compare_selection(
        &self,
        about: &str,
        observation: &LexicalObservation,
    ) -> Result<ShadowReport, String> {
        let started = std::time::Instant::now();
        let Some(stats) = self.sidecar.stats(about)? else {
            return Ok(ShadowReport::not_comparable("about not built"));
        };
        let held = self.sidecar.fingerprints(about, observation.aliased())?;
        let mut report = ShadowReport::not_comparable("selection");
        report.documents = observation.documents();
        report.selection_language = stats.language.as_deref() != observation.language();
        report.selection_rows = observation
            .fingerprints()
            .iter()
            .filter(|(doc, fingerprint)| held.get(*doc) != Some(*fingerprint))
            .count() as u64;
        report.elapsed_us = started.elapsed().as_micros() as u64;
        Ok(report)
    }
}
