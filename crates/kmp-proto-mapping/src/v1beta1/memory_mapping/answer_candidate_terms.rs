use std::collections::BTreeSet;

use kmp_domain::SearchSummary;
use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_recall_context::AnswerRecallContext;
use super::answer_selection::{answer_context_refs, is_retrieval_provenance};
use super::search_terms::{informative_term_counts, informative_terms};
use super::term_counts::TermCounts;

pub(super) struct AnswerCandidateTerms {
    /// What the candidate says: its text, and the writer's English rendering
    /// of it when one passed the lint. They are one content for ranking — a
    /// reader shown the entry is shown both — and are kept apart in `text`
    /// and `summary` only so a citation can say which of them the question
    /// landed on.
    pub(super) content: BTreeSet<String>,
    pub(super) content_counts: TermCounts,
    /// The text alone.
    pub(super) text: BTreeSet<String>,
    /// The writer's rendering alone; empty when there is none or it failed
    /// the lint.
    pub(super) summary: BTreeSet<String>,
    pub(super) direct_counts: TermCounts,
    pub(super) claim: BTreeSet<String>,
    pub(super) relation_why: BTreeSet<String>,
    pub(super) relation: BTreeSet<String>,
    pub(super) searchable: BTreeSet<String>,
}

impl AnswerCandidateTerms {
    pub(super) fn from_evidence(item: &MemoryEvidence, context: &AnswerRecallContext) -> Self {
        let morphology = &context.morphology;
        let summary_text = search_summary(item);
        let text = informative_terms(&item.text, morphology);
        let summary = summary_text
            .map(|summary| informative_terms(summary, morphology))
            .unwrap_or_default();
        let content_text = match summary_text {
            Some(summary) => format!("{} {}", item.text, summary),
            None => item.text.clone(),
        };
        let content = informative_terms(&content_text, morphology);
        let content_counts = informative_term_counts(&content_text, morphology);
        let mut direct_text = format!("{} {}", content_text, item.source);
        direct_text.push(' ');
        direct_text.push_str(&item.id);
        for supported_ref in &item.supports {
            direct_text.push(' ');
            direct_text.push_str(supported_ref);
        }
        for (key, value) in &item.metadata {
            // The summary is content, above, or nothing at all; the keys the
            // ranker writes about how a candidate was retrieved are read by
            // people and never searched.
            if is_retrieval_provenance(key) || key == SearchSummary::METADATA_KEY {
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
            text,
            summary,
            direct_counts,
            claim,
            relation_why,
            relation,
            searchable,
        }
    }
}

/// The writer's English rendering of a memory, when it passes the same
/// reading the ingest warned with.
///
/// A summary that dropped an identifier or arrived in the wrong language
/// stays visible beside the memory and is searched by nobody, whoever wrote
/// it and however it arrived.
fn search_summary(item: &MemoryEvidence) -> Option<&str> {
    let summary = item.metadata.get(SearchSummary::METADATA_KEY)?;
    SearchSummary::lint(&item.text, summary)
        .ok()
        .map(|_| summary.as_str())
}
