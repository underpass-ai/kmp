use std::sync::{Arc, Mutex};

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::AnswerCandidateTerms;
use super::lexical_collection::LexicalCollection;
use super::lexical_index_identity::LexicalIndexIdentity;

/// One bounded, immutable lexical collection per serving backend.
///
/// Missing revisions, oversized collections and poisoned locks use the ordinary
/// build path. Publication is short and synchronous; construction and ranking
/// never hold the mutex. Concurrent misses may build independently, but cannot
/// reuse a different revision, selection or term-count configuration.
#[derive(Default)]
pub struct LexicalIndexCache {
    current: Mutex<Option<(LexicalIndexIdentity, Arc<LexicalCollection>)>>,
}

const MAX_DOCUMENTS: usize = 8192;
const RETENTION_ALLOWANCE: usize = 32 * 1024 * 1024;
/// The collection itself, as `LexicalCollection::retained_bytes` counts it.
const COLLECTION_OVERHEAD: usize = 256;
/// One document's slot in the witness, rounded up.
const DOCUMENT_OVERHEAD: usize = 256;
/// Tree overhead of one term: 128 bytes in the witness and 128 in its field.
const TERM_OVERHEAD: usize = 256;

impl LexicalIndexCache {
    pub(super) fn collection(
        &self,
        identity: Option<&LexicalIndexIdentity>,
        prepared: &[(MemoryEvidence, AnswerCandidateTerms)],
    ) -> Arc<LexicalCollection> {
        if let Some(identity) = identity
            && let Ok(cache) = self.current.lock()
            && let Some((stored, collection)) = cache.as_ref()
            && stored == identity
            && collection.matches(prepared)
        {
            return Arc::clone(collection);
        }
        let retain = identity.is_some_and(|key| cacheable(key, prepared));
        let collection = Arc::new(LexicalCollection::build(prepared, retain));
        let retain = retain
            && identity.is_some_and(|key| {
                collection
                    .retained_bytes()
                    .saturating_add(key.retained_bytes())
                    <= RETENTION_ALLOWANCE
            });
        if let Ok(mut cache) = self.current.lock() {
            *cache = identity
                .filter(|_| retain)
                .map(|key| (key.clone(), Arc::clone(&collection)));
        }
        collection
    }
}

// Admission accounts conservatively for what `LexicalCollection` retains: the
// equality witness (a copy of every document's two term maps) and both field
// maps, each term charged 128 bytes of tree overhead plus its bytes, once in
// the witness and once in its field, as `retained_bytes` measures them. No
// co-occurrence pairs or neighbours are retained: associations are counted per
// question from the direct field. This bounds retained cache material, not
// peak build allocations or process RSS.
fn cacheable(
    identity: &LexicalIndexIdentity,
    prepared: &[(MemoryEvidence, AnswerCandidateTerms)],
) -> bool {
    if prepared.len() > MAX_DOCUMENTS {
        return false;
    }
    let mut remaining = RETENTION_ALLOWANCE
        .checked_sub(identity.retained_bytes())
        .and_then(|n| n.checked_sub(COLLECTION_OVERHEAD));
    for (_, terms) in prepared {
        remaining = remaining.and_then(|n| n.checked_sub(DOCUMENT_OVERHEAD));
        for term in terms
            .content_counts
            .terms()
            .chain(terms.direct_counts.terms())
        {
            remaining = remaining.and_then(|n| n.checked_sub(term_charge(term)));
            if remaining.is_none() {
                return false;
            }
        }
    }
    remaining.is_some()
}

/// One term occurrence: its witness entry and its field entry.
fn term_charge(term: &str) -> usize {
    TERM_OVERHEAD.saturating_add(term.len().saturating_mul(2))
}

#[cfg(test)]
mod tests {
    use super::super::lexical_index_tests::{fixture, prepared};
    use super::*;
    use kmp_domain::TemporalSelection;

    /// Whatever admission lets in fits the allowance once built, so the
    /// post-build check is a guard and not the admission itself.
    #[test]
    fn admitted_collections_fit_the_allowance_they_were_admitted_under() {
        for (count, vocabulary) in [(12, 0), (24, 8), (64, 16), (256, 64), (32, 256)] {
            let result = fixture(count, vocabulary);
            let terms = prepared(&result);
            let identity = LexicalIndexIdentity::read(&result, &TemporalSelection::Frontier)
                .expect("identity");
            assert!(cacheable(&identity, &terms), "{count}x{vocabulary}");
            let collection = LexicalCollection::build(&terms, true);
            let charged = COLLECTION_OVERHEAD
                + terms
                    .iter()
                    .map(|(_, t)| {
                        DOCUMENT_OVERHEAD
                            + t.content_counts
                                .terms()
                                .chain(t.direct_counts.terms())
                                .map(|term| term_charge(term))
                                .sum::<usize>()
                    })
                    .sum::<usize>();
            assert!(
                collection.retained_bytes() <= charged,
                "{count}x{vocabulary}"
            );
        }
    }

    #[test]
    fn one_oversized_term_is_refused_before_the_build() {
        let result = fixture(24, 0);
        let mut terms = prepared(&result);
        let identity =
            LexicalIndexIdentity::read(&result, &TemporalSelection::Frontier).expect("identity");
        assert!(cacheable(&identity, &terms));
        terms[0]
            .1
            .direct_counts
            .insert("x".repeat(RETENTION_ALLOWANCE / 2));
        assert!(!cacheable(&identity, &terms));
    }
}
