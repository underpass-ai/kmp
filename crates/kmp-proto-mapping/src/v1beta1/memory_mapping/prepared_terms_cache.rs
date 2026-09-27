use std::sync::Mutex;

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::AnswerCandidateTerms;

/// The terms one ask read its candidates with, kept for the next reading of
/// the same candidates in the same ask (P10).
///
/// A doubt band reads the question before its judge is asked, and the
/// answer reads it again when verdicts come back, because they act inside
/// the anchored gate. With the verdict book warm the judge answers at once,
/// and what the band added was that second reading: every candidate's terms
/// read again (+100–190 ms on the real store). The terms depend only on the
/// candidate and on the setup both readings share, so the second reading
/// takes them from here, for exactly the candidates and reading (plain or
/// aliased) they were read with.
#[derive(Debug, Default)]
pub(super) struct PreparedTermsCache {
    kept: Mutex<Option<Kept>>,
}

#[derive(Debug)]
struct Kept {
    aliased: bool,
    evidence: Vec<MemoryEvidence>,
    terms: Vec<AnswerCandidateTerms>,
}

impl PreparedTermsCache {
    /// The terms read for exactly `evidence` under this reading, if kept:
    /// cloned while the band still reads (`keep`), taken by the answer's
    /// reading, the last one.
    pub(super) fn get(
        &self,
        evidence: &[MemoryEvidence],
        aliased: bool,
        keep: bool,
    ) -> Option<Vec<AnswerCandidateTerms>> {
        let mut kept = self.kept.lock().ok()?;
        let matches = kept
            .as_ref()
            .is_some_and(|kept| kept.aliased == aliased && kept.evidence == evidence);
        if !matches {
            return None;
        }
        if keep {
            kept.as_ref().map(|kept| kept.terms.clone())
        } else {
            kept.take().map(|kept| kept.terms)
        }
    }

    /// Keeps the terms read for `evidence`.
    pub(super) fn put(
        &self,
        evidence: Vec<MemoryEvidence>,
        aliased: bool,
        terms: Vec<AnswerCandidateTerms>,
    ) {
        if let Ok(mut kept) = self.kept.lock() {
            *kept = Some(Kept {
                aliased,
                evidence,
                terms,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::answer_recall_context::AnswerRecallContext;
    use super::*;

    fn evidence(text: &str) -> MemoryEvidence {
        MemoryEvidence {
            id: format!("detail:{text}"),
            text: text.to_string(),
            ..MemoryEvidence::default()
        }
    }

    #[test]
    fn the_band_keeps_the_terms_and_the_answer_takes_them_back_once() {
        let context = AnswerRecallContext::default();
        let pool = vec![evidence("the valve froze"), evidence("the pump started")];
        let terms = pool
            .iter()
            .map(|item| AnswerCandidateTerms::from_evidence(item, &context))
            .collect::<Vec<_>>();
        let cache = PreparedTermsCache::default();
        assert!(cache.get(&pool, false, true).is_none());
        cache.put(pool.clone(), false, terms);
        // Another reading, or other candidates, read their own.
        assert!(cache.get(&pool, true, true).is_none());
        assert!(cache.get(&pool[..1], false, true).is_none());
        // The band reads them again; the answer takes them, once.
        assert_eq!(
            cache.get(&pool, false, true).map(|terms| terms.len()),
            Some(2)
        );
        assert_eq!(
            cache.get(&pool, false, false).map(|terms| terms.len()),
            Some(2)
        );
        assert!(cache.get(&pool, false, false).is_none());
    }
}
