use std::collections::BTreeSet;

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_recall_context::AnswerRecallContext;
use super::answer_selection::{answer_context_refs, is_retrieval_provenance};
use super::search_terms::{informative_term_counts, informative_terms};
use super::term_counts::TermCounts;

pub(super) struct AnswerCandidateTerms {
    pub(super) content: BTreeSet<String>,
    pub(super) content_counts: TermCounts,
    pub(super) direct_counts: TermCounts,
    pub(super) claim: BTreeSet<String>,
    pub(super) relation_why: BTreeSet<String>,
    pub(super) relation: BTreeSet<String>,
    pub(super) searchable: BTreeSet<String>,
}

impl AnswerCandidateTerms {
    pub(super) fn from_evidence(item: &MemoryEvidence, context: &AnswerRecallContext) -> Self {
        let morphology = &context.morphology;
        let content = informative_terms(&item.text, morphology);
        let content_counts = informative_term_counts(&item.text, morphology);
        let mut direct_text = format!("{} {}", item.text, item.source);
        direct_text.push(' ');
        direct_text.push_str(&item.id);
        for supported_ref in &item.supports {
            direct_text.push(' ');
            direct_text.push_str(supported_ref);
        }
        for (key, value) in &item.metadata {
            if is_retrieval_provenance(key) {
                continue;
            }
            direct_text.push(' ');
            direct_text.push_str(key);
            direct_text.push(' ');
            direct_text.push_str(value);
        }
        let direct_counts = informative_term_counts(&direct_text, morphology);
        let direct = informative_terms(&direct_text, morphology);

        let mut claim = item
            .supports
            .iter()
            .flat_map(|supported_ref| informative_terms(supported_ref, morphology))
            .collect::<BTreeSet<_>>();
        for selected_ref in answer_context_refs(item) {
            if let Some(detail_terms) = context.details_by_ref.get(&selected_ref) {
                claim.extend(detail_terms.iter().cloned());
            }
        }

        let relationships = context.relationships_for(item);
        let relation_why = relationships
            .iter()
            .flat_map(|relationship| relationship.why_terms.iter().cloned())
            .collect::<BTreeSet<_>>();
        let relation = relationships
            .iter()
            .flat_map(|relationship| relationship.searchable_terms())
            .collect::<BTreeSet<_>>();
        let searchable = direct
            .iter()
            .chain(&claim)
            .chain(&relation)
            .cloned()
            .collect();

        Self {
            content,
            content_counts,
            direct_counts,
            claim,
            relation_why,
            relation,
            searchable,
        }
    }
}
