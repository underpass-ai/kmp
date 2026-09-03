use std::collections::BTreeSet;

use kmp_domain::SearchSummary;
use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_recall_context::AnswerRecallContext;
use super::answer_selection::{answer_context_refs, is_retrieval_provenance};
use super::search_terms::{informative_keys, informative_terms};
use super::term_counts::TermCounts;

pub(super) struct AnswerCandidateTerms {
    /// What the candidate says: its text, and the writer's English rendering
    /// of it when one passed the lint. They are one content for ranking — a
    /// reader shown the entry is shown both — and are kept apart in `text`
    /// and `summary` only so a citation can say which of them the question
    /// landed on. The text folds in the store's language and the summary in
    /// the kernel's search language, so an English question reaches the
    /// English summary even when the store's own language cannot be read.
    pub(super) content: BTreeSet<String>,
    pub(super) content_counts: TermCounts,
    /// The text alone, folded in the store's language.
    pub(super) text: BTreeSet<String>,
    /// The writer's rendering alone, folded in the kernel's search language;
    /// empty when there is none or it failed the lint.
    pub(super) summary: BTreeSet<String>,
    pub(super) direct_counts: TermCounts,
    pub(super) claim: BTreeSet<String>,
    pub(super) relation_why: BTreeSet<String>,
    pub(super) relation: BTreeSet<String>,
    pub(super) searchable: BTreeSet<String>,
}

impl AnswerCandidateTerms {
    pub(super) fn from_evidence(item: &MemoryEvidence, context: &AnswerRecallContext) -> Self {
        let text_morphology = &context.morphology;
        let summary_morphology = &context.summary_morphology;
        let summary_text = search_summary(item);

        let text = informative_terms(&item.text, text_morphology);
        let summary = summary_text
            .map(|summary| informative_terms(summary, summary_morphology))
            .unwrap_or_default();

        // The content is the text and the rendering as one bag, each folded in
        // its own language: the text keys under the store's stemmer, the
        // summary keys under the kernel's. A single stemmer over both would be
        // the store's, and would leave an English summary unstemmed in a
        // Spanish store.
        let content_keys = || {
            informative_keys(&item.text, text_morphology).chain(
                summary_text
                    .into_iter()
                    .flat_map(|summary| informative_keys(summary, summary_morphology)),
            )
        };
        let content = content_keys().collect::<BTreeSet<_>>();
        let content_counts = content_keys().collect::<TermCounts>();

        // The identifiers and metadata a candidate is addressed by fold in the
        // store's language, like its text; the summary is already in the bag.
        let mut direct_extra = format!("{} {}", item.source, item.id);
        for supported_ref in &item.supports {
            direct_extra.push(' ');
            direct_extra.push_str(supported_ref);
        }
        for (key, value) in &item.metadata {
            // The summary is content, above, or nothing at all; the keys the
            // ranker writes about how a candidate was retrieved are read by
            // people and never searched.
            if is_retrieval_provenance(key) || key == SearchSummary::METADATA_KEY {
                continue;
            }
            direct_extra.push(' ');
            direct_extra.push_str(key);
            direct_extra.push(' ');
            direct_extra.push_str(value);
        }
        let direct_counts = content_keys()
            .chain(informative_keys(&direct_extra, text_morphology))
            .collect::<TermCounts>();
        let direct = content
            .iter()
            .cloned()
            .chain(informative_keys(&direct_extra, text_morphology))
            .collect::<BTreeSet<_>>();

        let mut claim = item
            .supports
            .iter()
            .flat_map(|supported_ref| informative_terms(supported_ref, text_morphology))
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
