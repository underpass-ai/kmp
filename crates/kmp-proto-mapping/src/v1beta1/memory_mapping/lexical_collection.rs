use super::answer_candidate_terms::AnswerCandidateTerms;
use super::association_index::AssociationIndex;
use super::lexical_field::LexicalField;
use super::term_counts::TermCounts;
use kmp_proto::v1beta1::MemoryEvidence;

/// Query-independent BM25 field statistics and co-occurrence associations.
/// Retained term counts are an exact equality witness, not truncated tokens or
/// hashes. Canonical evidence and ranked query results are never retained here.
pub(super) struct LexicalCollection {
    pub(super) content: LexicalField,
    pub(super) direct: LexicalField,
    pub(super) associations: AssociationIndex,
    documents: Vec<(TermCounts, TermCounts)>,
}

impl LexicalCollection {
    pub(super) fn retained_bytes(&self) -> usize {
        256 + self.content.retained_bytes()
            + self.direct.retained_bytes()
            + self.associations.retained_bytes()
            + self.documents.capacity() * std::mem::size_of::<(TermCounts, TermCounts)>()
            + self
                .documents
                .iter()
                .map(|(content, direct)| content.retained_bytes() + direct.retained_bytes())
                .sum::<usize>()
    }

    pub(super) fn build(prepared: &[(MemoryEvidence, AnswerCandidateTerms)], retain: bool) -> Self {
        Self {
            content: LexicalField::build(prepared.iter().map(|(_, terms)| &terms.content_counts)),
            direct: LexicalField::build(prepared.iter().map(|(_, terms)| &terms.direct_counts)),
            associations: AssociationIndex::build(
                prepared.iter().map(|(_, terms)| &terms.direct_counts),
            ),
            documents: if retain {
                prepared
                    .iter()
                    .map(|(_, terms)| (terms.content_counts.clone(), terms.direct_counts.clone()))
                    .collect()
            } else {
                Vec::new()
            },
        }
    }

    pub(super) fn matches(&self, prepared: &[(MemoryEvidence, AnswerCandidateTerms)]) -> bool {
        self.documents.len() == prepared.len()
            && self
                .documents
                .iter()
                .zip(prepared)
                .all(|((content, direct), (_, terms))| {
                    content == &terms.content_counts && direct == &terms.direct_counts
                })
    }
}
