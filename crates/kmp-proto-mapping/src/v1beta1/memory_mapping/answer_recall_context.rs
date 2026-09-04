use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{KmpBundle, RelationSemanticClass, RelationSignal, SearchSummary};
use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_selection::answer_context_refs;
use super::bundle_views::persisted_memory_metadata;
use super::candidate_temporal_state::CandidateTemporalState;
use super::memory_lifecycle::MemoryLifecycle;
use super::morphology::Morphology;
use super::reach_graph::ReachGraph;
use super::relation_direction::RelationDirection;
use super::relation_feature::RelationFeature;
use super::search_terms::informative_terms;

/// One candidate keeps only the relations best able to explain it, so a
/// high-degree node cannot make ranking grow with its degree.
const MAX_RELATION_FEATURES_PER_CANDIDATE: usize = 16;

#[derive(Default)]
pub(super) struct AnswerRecallContext {
    pub(super) details_by_ref: BTreeMap<String, BTreeSet<String>>,
    pub(super) relationships_by_ref: BTreeMap<String, Vec<RelationFeature>>,
    pub(super) lifecycle: MemoryLifecycle,
    pub(super) reach_graph: ReachGraph,
    /// Read once, then used for both sides of every comparison. Stemming a
    /// question by one language's rules and the memory by another's would
    /// split families rather than join them, so the question, the text, the
    /// summary, the details and the relations all fold with this one.
    ///
    /// It is the store's own language when that can be read. When it cannot —
    /// a store of two languages reads as none — it is the kernel's search
    /// language, but only if the store carries English summaries to land on:
    /// a mixed store used to stem nothing, which left an English plural short
    /// of the singular in an English summary. A store with no summary keeps
    /// exact matching rather than being stemmed by rules its text never was.
    pub(super) morphology: Morphology,
}

impl AnswerRecallContext {
    /// The context standing on a lifecycle read where the recall stands: at
    /// the memory's frontier, or at an instant the caller named.
    pub(super) fn from_bundle_with_lifecycle(
        bundle: &KmpBundle,
        lifecycle: MemoryLifecycle,
    ) -> Self {
        let morphology = search_morphology(bundle);
        let details_by_ref = bundle
            .node_details()
            .iter()
            .map(|detail| {
                (
                    detail.node_id().to_string(),
                    informative_terms(detail.detail(), &morphology),
                )
            })
            .collect();
        let mut relationships_by_ref = BTreeMap::<String, Vec<_>>::new();
        for relationship in bundle
            .relationships()
            .iter()
            .filter(|relationship| relationship_is_explanatory(relationship))
        {
            let explanation = relationship.explanation();
            let endpoint_terms = informative_terms(
                &format!(
                    "{} {}",
                    relationship.source_node_id(),
                    relationship.target_node_id()
                ),
                &morphology,
            );
            let relation_evidence = explanation.evidence().unwrap_or_default();
            let evidence_terms = informative_terms(relation_evidence, &morphology);
            // A rationale can improve ranking only when the relation carries
            // its own evidence. It remains context, never a freestanding fact.
            let why_terms = if relation_evidence.trim().is_empty() {
                BTreeSet::new()
            } else {
                informative_terms(
                    &format!(
                        "{} {}",
                        explanation.rationale().unwrap_or_default(),
                        explanation.motivation().unwrap_or_default()
                    ),
                    &morphology,
                )
            };
            let relation_terms = informative_terms(relationship.relationship_type(), &morphology);
            let signal =
                RelationSignal::read(relationship.relationship_type(), explanation).weight();

            let outgoing = RelationFeature {
                rel: relationship.relationship_type().to_string(),
                semantic_class: *explanation.semantic_class(),
                signal,
                direction: RelationDirection::Outgoing,
                other_endpoint_ref: relationship.target_node_id().to_string(),
                endpoint_terms: endpoint_terms.clone(),
                why_terms: why_terms.clone(),
                evidence_terms: evidence_terms.clone(),
                relation_terms: relation_terms.clone(),
            };
            relationships_by_ref
                .entry(relationship.source_node_id().to_string())
                .or_default()
                .push(outgoing);

            if relationship.target_node_id() != relationship.source_node_id() {
                relationships_by_ref
                    .entry(relationship.target_node_id().to_string())
                    .or_default()
                    .push(RelationFeature {
                        rel: relationship.relationship_type().to_string(),
                        semantic_class: *explanation.semantic_class(),
                        signal,
                        direction: RelationDirection::Incoming,
                        other_endpoint_ref: relationship.source_node_id().to_string(),
                        endpoint_terms,
                        why_terms,
                        evidence_terms,
                        relation_terms,
                    });
            }
        }

        for relationships in relationships_by_ref.values_mut() {
            relationships.sort_by(RelationFeature::stable_cmp);
            relationships.dedup();
            relationships.truncate(MAX_RELATION_FEATURES_PER_CANDIDATE);
        }

        Self {
            details_by_ref,
            relationships_by_ref,
            lifecycle,
            reach_graph: ReachGraph::from_bundle(bundle),
            morphology,
        }
    }

    /// The `proof.expired` list for the lifecycle this context stands on.
    pub(super) fn expired_memories(&self) -> Vec<kmp_proto::v1beta1::ExpiredMemory> {
        self.lifecycle.expired_memories()
    }

    pub(super) fn relationships_for<'a>(
        &'a self,
        item: &MemoryEvidence,
    ) -> Vec<&'a RelationFeature> {
        let mut relationships = Vec::new();
        if let Some(evidence_ref) = item.id.strip_prefix("detail:")
            && let Some(direct) = self.relationships_by_ref.get(evidence_ref)
        {
            relationships.extend(direct);
        }
        for supported_ref in &item.supports {
            if let Some(semantic) = self.relationships_by_ref.get(supported_ref) {
                // Do not follow a claim's `supports` edges to sibling evidence.
                // That would make high-degree claims leak unrelated vocabulary
                // and turn candidate construction into quadratic work.
                relationships.extend(
                    semantic
                        .iter()
                        .filter(|relationship| relationship.rel != "supports"),
                );
            }
        }
        relationships.sort_by(|left, right| left.stable_cmp(right));
        relationships.dedup();
        relationships.truncate(MAX_RELATION_FEATURES_PER_CANDIDATE);
        relationships
    }

    pub(super) fn temporal_state(&self, item: &MemoryEvidence) -> CandidateTemporalState {
        let refs = answer_context_refs(item);
        if refs
            .iter()
            .any(|selected_ref| self.lifecycle.is_superseded(selected_ref))
        {
            CandidateTemporalState::Superseded
        } else if refs
            .iter()
            .any(|selected_ref| self.lifecycle.is_expired(selected_ref))
        {
            CandidateTemporalState::Expired
        } else {
            CandidateTemporalState::CurrentOrUnspecified
        }
    }

    /// How recent a candidate is against the store's own present, in coarse
    /// buckets so a few seconds never outrank a better text match.
    ///
    /// An entry with no time is not treated as ancient: it ranks with old
    /// material rather than below it, because an absent clock is a silence,
    /// not a claim of age.
    pub(super) fn recency_rank(&self, item: &MemoryEvidence) -> u32 {
        self.lifecycle.recency_rank(item.time.as_ref())
    }
}

/// The one stemmer every comparison in this bundle uses, on both sides.
///
/// The store's own language, read from its entries, details and relations
/// as one text. When that cannot be read — a store of two languages reads as
/// none — the kernel's search language, but only if the store carries an
/// English summary for a question to land on; otherwise none, which leaves
/// every word exactly as written rather than stemmed by rules the store's
/// text never was.
pub(super) fn search_morphology(bundle: &KmpBundle) -> Morphology {
    let store_language = Morphology::read_language(
        std::iter::once(bundle.root_node())
            .chain(bundle.neighbor_nodes())
            .map(|node| node.summary())
            .chain(bundle.node_details().iter().map(|detail| detail.detail()))
            .chain(bundle.relationships().iter().flat_map(|relationship| {
                let explanation = relationship.explanation();
                [
                    explanation.rationale().unwrap_or_default(),
                    explanation.motivation().unwrap_or_default(),
                    explanation.evidence().unwrap_or_default(),
                ]
            })),
    );
    let search_language = store_language.as_deref().or_else(|| {
        bundle_carries_search_summary(bundle).then_some(kmp_domain::language::KERNEL_LANGUAGE)
    });
    Morphology::for_language(search_language)
}

/// Whether any memory in the bundle carries an English search summary that
/// passes the same lint the ranker searches it under.
///
/// It decides one thing: whether a store whose own language could not be read
/// folds in the kernel's search language. Only a store with such a summary has
/// an English surface for a question to land on; one without keeps exact
/// matching.
fn bundle_carries_search_summary(bundle: &KmpBundle) -> bool {
    std::iter::once(bundle.root_node())
        .chain(bundle.neighbor_nodes())
        .any(|node| {
            persisted_memory_metadata(node.properties())
                .get(SearchSummary::METADATA_KEY)
                .is_some_and(|summary| SearchSummary::lint(node.summary(), summary).is_ok())
        })
}

pub(super) fn relationship_is_explanatory(relationship: &kmp_domain::BundleRelationship) -> bool {
    match relationship.explanation().semantic_class() {
        RelationSemanticClass::Causal
        | RelationSemanticClass::Motivational
        | RelationSemanticClass::Constraint => true,
        RelationSemanticClass::Evidential => relationship.relationship_type() != "supports",
        RelationSemanticClass::Structural | RelationSemanticClass::Procedural => false,
    }
}
