use kmp_proto_mapping::v1beta1::LexicalObservation;

use super::shadow_report::ShadowReport;
use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use crate::serving::ports::lexical_candidates::LexicalCandidates;

/// Compares the sidecar's view of one about with what an ask's ranker
/// measured over the same candidates: the integers BM25 reads (N, Σlen per
/// field, df of every weighted term, tf and length of every candidate, by
/// fingerprint) and whether the postings of the weighted terms reach every
/// candidate that could score. The ranker's answer is never touched.
pub(super) struct ShadowComparison<'s> {
    sidecar: &'s SqliteLexicalSidecar,
    aliased: bool,
}

impl<'s> ShadowComparison<'s> {
    pub(super) fn new(sidecar: &'s SqliteLexicalSidecar, aliased: bool) -> Self {
        Self { sidecar, aliased }
    }

    pub(super) fn compare(
        &self,
        about: &str,
        observation: &LexicalObservation,
    ) -> Result<ShadowReport, String> {
        let started = std::time::Instant::now();
        if observation.aliased() != self.aliased {
            return Ok(ShadowReport::not_comparable("another reading"));
        }
        let (Some(totals), Some(stats)) = (self.sidecar.totals(about)?, self.sidecar.stats(about)?)
        else {
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
        let frequencies = LexicalCandidates::frequencies(self.sidecar, about, &weighted)?;
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
                Some(row) if seen.get(doc) != Some(&row.fingerprint()) => {
                    report.row_differences += 1;
                }
                Some(_) => {}
            }
        }
        // Every other candidate, through the digest of all their rows.
        if stats.rows_digest != observation.rows_digest() && report.row_differences == 0 {
            let held = self.sidecar.fingerprints(about)?;
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
}
