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

// Admission accounts conservatively for the equality witness, both field maps,
// the three retained neighbours per term, String capacities and tree overhead.
// This bounds retained cache material, not peak build allocations or process RSS.
fn cacheable(
    identity: &LexicalIndexIdentity,
    prepared: &[(MemoryEvidence, AnswerCandidateTerms)],
) -> bool {
    if prepared.len() > MAX_DOCUMENTS {
        return false;
    }
    let mut remaining = RETENTION_ALLOWANCE.checked_sub(identity.retained_bytes());
    for (_, terms) in prepared {
        remaining = remaining.and_then(|n| n.checked_sub(256));
        for term in terms
            .content_counts
            .terms()
            .chain(terms.direct_counts.terms())
        {
            remaining = remaining.and_then(|n| {
                n.checked_sub(512usize.saturating_add(term.len().saturating_mul(16)))
            });
            if remaining.is_none() {
                return false;
            }
        }
    }
    remaining.is_some()
}
