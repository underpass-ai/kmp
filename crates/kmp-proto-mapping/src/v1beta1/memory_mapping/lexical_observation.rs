use std::collections::{BTreeMap, BTreeSet};

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate_terms::AnswerCandidateTerms;
use super::lexical_collection::LexicalCollection;
use super::lexical_row::LexicalRow;

/// What the ranker measured over one ask's candidates, kept so the lexical
/// sidecar can be compared with it (DESIGN L6, shadow mode): the integers
/// BM25 depends on and which candidates the question could score.
///
/// Recorded once per ask, from the first collection the ranker builds; every
/// later ranking of the same ask reads the same candidates.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LexicalObservation {
    language: Option<String>,
    aliased: bool,
    documents: u64,
    content_length: i64,
    direct_length: i64,
    expanded: u64,
    expansion_length: i64,
    rows_digest: u64,
    fingerprints: BTreeMap<String, u64>,
    frequencies: BTreeMap<String, (u64, u64)>,
    scored: BTreeSet<String>,
}

impl LexicalObservation {
    pub(super) fn read(
        prepared: &[(MemoryEvidence, AnswerCandidateTerms)],
        collection: &LexicalCollection,
        weighted: impl IntoIterator<Item = String>,
        language: Option<String>,
        aliased: bool,
    ) -> Self {
        let mut observation = Self {
            language,
            aliased,
            documents: prepared.len() as u64,
            ..Self::default()
        };
        for (item, terms) in prepared {
            observation.content_length += terms.content_counts.length() as i64;
            observation.direct_length += terms.direct_counts.length() as i64;
            let expansion = terms.expansion_counts.length() as i64;
            if expansion > 0 {
                observation.expanded += 1;
                observation.expansion_length += expansion;
            }
            let fingerprint = LexicalRow::fingerprint_of_counts(
                &terms.content_counts,
                &terms.direct_counts,
                &terms.expansion_counts,
            );
            observation.rows_digest = observation.rows_digest.wrapping_add(fingerprint);
            observation
                .fingerprints
                .insert(item.id.clone(), fingerprint);
        }
        let weighted = weighted.into_iter().collect::<BTreeSet<_>>();
        for (item, terms) in prepared {
            // A candidate its expansions alone carry to the question can be
            // rescued (P15), so a generator must reach it too.
            if weighted.iter().any(|term| {
                terms.direct_counts.count(term) > 0
                    || terms.content_counts.count(term) > 0
                    || terms.expansion_counts.count(term) > 0
            }) {
                observation.scored.insert(item.id.clone());
            }
        }
        observation.frequencies = weighted
            .into_iter()
            .map(|term| {
                let frequency = (
                    collection.content.document_frequency(&term) as u64,
                    collection.direct.document_frequency(&term) as u64,
                );
                (term, frequency)
            })
            .collect();
        observation
    }

    /// The language the candidates were stemmed in.
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Whether the candidates were read with their alias terms.
    pub fn aliased(&self) -> bool {
        self.aliased
    }

    /// N: how many candidates the collection was measured over.
    pub fn documents(&self) -> u64 {
        self.documents
    }

    /// Σ length over the content field and over the direct field.
    pub fn lengths(&self) -> (i64, i64) {
        (self.content_length, self.direct_length)
    }

    /// How many candidates carry judged expansions, and Σ their length.
    pub fn expansions(&self) -> (u64, i64) {
        (self.expanded, self.expansion_length)
    }

    /// The wrapping sum of every candidate's row fingerprint: equal digests
    /// mean equal tf and length for every candidate, up to a collision.
    pub fn rows_digest(&self) -> u64 {
        self.rows_digest
    }

    /// Each candidate's row fingerprint, by candidate id.
    pub fn fingerprints(&self) -> &BTreeMap<String, u64> {
        &self.fingerprints
    }

    /// df in the content and the direct field of every term the question
    /// weighed (its own words, their associations and bridged words).
    pub fn frequencies(&self) -> &BTreeMap<String, (u64, u64)> {
        &self.frequencies
    }

    /// The candidates that carry a weighted term in a field or in their
    /// expansions, so could score above zero or be rescued: the ones a
    /// candidate generator must never miss.
    pub fn scored(&self) -> &BTreeSet<String> {
        &self.scored
    }
}
