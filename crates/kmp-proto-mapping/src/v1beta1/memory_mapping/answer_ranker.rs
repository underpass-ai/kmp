use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use kmp_application::MemoryAnswerPolicy;
use kmp_domain::{KmpBundle, RelationSemanticClass, RelationSignal};
use kmp_proto::v1beta1::{MemoryConfidence, MemoryEvidence};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use super::lexical_index::{LexicalField, TermCounts, ranked_score};
use super::memory_lifecycle::MemoryLifecycle;
use super::question_intent::QuestionIntent;
use super::relation_reach::{ReachGraph, RelationReach};

pub(super) const ANSWER_CORE_LIMIT: usize = 5;

/// Metadata key marking a candidate the question never matched on its own
/// text, reached by walking proven relations out from one that did.
pub(super) const REACHED_BY_KEY: &str = "reached_by";
pub(super) const REACHED_BY_RELATION: &str = "relation";

const MAX_RELATION_FEATURES_PER_CANDIDATE: usize = 16;
const MAX_RERANK_CANDIDATES: usize = 64;
/// How far retrieval may walk from something the question actually matched.
/// Two hops covers `symptom → decision → constraint`, the shape a root-cause
/// question needs, without opening the whole neighbourhood.
const MAX_REACH_HOPS: u32 = 2;
const MAX_REACHED_REFS: usize = 8;
const MAX_RESCUED_CANDIDATES: usize = 5;

/// Deterministic, graph-aware reranker for stored entry text and evidence used
/// by `kmp_ask`.
///
/// Direct stored text establishes eligibility. Graph detail and explanatory
/// relationships can then improve an eligible candidate's position, but can
/// never promote unrelated evidence into an answer.
#[derive(Default)]
pub(super) struct AnswerEvidenceRanker {
    context: AnswerRecallContext,
}

impl AnswerEvidenceRanker {
    pub(super) fn from_bundle(bundle: &KmpBundle) -> Self {
        Self {
            context: AnswerRecallContext::from_bundle(bundle),
        }
    }

    pub(super) fn rank(
        &self,
        question: &str,
        policy: MemoryAnswerPolicy,
        evidence: Vec<MemoryEvidence>,
    ) -> Vec<MemoryEvidence> {
        let question_terms = informative_terms(question);
        if question_terms.is_empty() {
            let mut evidence = evidence;
            evidence.sort_by_key(stable_evidence_key);
            return evidence;
        }

        let strict_focus = match policy {
            MemoryAnswerPolicy::EvidenceOrUnknown | MemoryAnswerPolicy::ShowConflicts => {
                let terms = strict_answer_focus_terms(question);
                let required_matches = (concept_count(&terms) * 2).div_ceil(3);
                Some((terms, required_matches))
            }
            MemoryAnswerPolicy::BestEffort => None,
        };
        let diversity_focus_terms = strict_focus
            .as_ref()
            .map(|(terms, _)| terms.clone())
            .unwrap_or_default();
        let intent = QuestionIntent::read(question);

        // BM25 needs a collection before it can weigh anything, so every
        // candidate's terms are read once, up front. The collection is this
        // question's own candidates: inside an about where every entry says
        // `deploy`, that word earns nothing, and only a measurement taken
        // here can know it.
        let prepared = evidence
            .into_iter()
            .map(|item| {
                let terms = AnswerCandidateTerms::from_evidence(&item, &self.context);
                (item, terms)
            })
            .collect::<Vec<_>>();
        let lexicon = Lexicon::build(question, &prepared);

        let mut candidates = Vec::new();
        let mut rejected = Vec::new();
        for (item, terms) in prepared {
            match AnswerCandidate::eligible(
                item,
                terms,
                &question_terms,
                strict_focus.as_ref(),
                &lexicon,
                &intent,
                &self.context,
            ) {
                Ok(candidate) => candidates.push(candidate),
                Err(item) => rejected.push(*item),
            }
        }

        candidates.sort_by(|left, right| {
            right
                .relevance
                .cmp(&left.relevance)
                .then_with(|| left.stable_key.cmp(&right.stable_key))
        });
        let ranked = diversify_candidates(&question_terms, &diversity_focus_terms, candidates);
        let mut answer = prioritize_distinct_claims(ranked)
            .into_iter()
            .map(|candidate| candidate.item)
            .collect::<Vec<_>>();

        answer.extend(self.reached_candidates(&answer, rejected));
        answer
    }

    /// Rescues what the question could not match on words but the graph can
    /// prove is connected to something it did.
    ///
    /// The ranker's standing rule is that direct stored text establishes
    /// eligibility and the graph may only improve a position. That rule is
    /// what keeps an unrelated memory out of an answer, and it stays: these
    /// candidates arrive after every eligible one and carry the mark that
    /// keeps them out of the answer core. What changes is that a memory
    /// causally upstream of a match is no longer unreachable just because it
    /// shares no vocabulary with the question.
    fn reached_candidates(
        &self,
        eligible: &[MemoryEvidence],
        rejected: Vec<MemoryEvidence>,
    ) -> Vec<MemoryEvidence> {
        if rejected.is_empty() || self.context.reach_graph.is_empty() {
            return Vec::new();
        }

        let seeds = eligible
            .iter()
            .flat_map(answer_context_refs)
            .collect::<BTreeSet<_>>();
        // A rescue must respect both lifecycles: a memory that was replaced,
        // and one that simply stopped applying, stay out however strong the
        // edge pointing at them.
        let mut blocked = self.context.lifecycle.superseded_refs().clone();
        blocked.extend(self.context.lifecycle.expired_refs().cloned());
        blocked.extend(seeds.iter().cloned());

        let reached =
            self.context
                .reach_graph
                .reach_from(&seeds, &blocked, MAX_REACH_HOPS, MAX_REACHED_REFS);
        if reached.is_empty() {
            return Vec::new();
        }

        let mut rescued = rejected
            .into_iter()
            .filter(|item| {
                self.context.temporal_state(item) == CandidateTemporalState::CurrentOrUnspecified
            })
            .filter_map(|item| {
                let hop = answer_context_refs(&item)
                    .iter()
                    .filter_map(|item_ref| reached.get(item_ref))
                    .min_by(|left, right| {
                        left.hops
                            .cmp(&right.hops)
                            .then_with(|| right.weight.cmp(&left.weight))
                    })?
                    .clone();
                Some((hop, item))
            })
            .collect::<Vec<_>>();

        rescued.sort_by(|(left_hop, left), (right_hop, right)| {
            left_hop
                .hops
                .cmp(&right_hop.hops)
                .then_with(|| right_hop.weight.cmp(&left_hop.weight))
                .then_with(|| stable_evidence_key(left).cmp(&stable_evidence_key(right)))
        });

        rescued
            .into_iter()
            .take(MAX_RESCUED_CANDIDATES)
            .map(|(hop, item)| mark_reached(item, &hop))
            .collect()
    }

    pub(super) fn confidence(
        &self,
        question: &str,
        evidence: &[MemoryEvidence],
    ) -> MemoryConfidence {
        if evidence.is_empty() {
            return MemoryConfidence::Unknown;
        }
        let question_terms = informative_terms(question);
        if question_terms.is_empty() {
            return MemoryConfidence::Low;
        }
        let best_matches = evidence
            .iter()
            .map(|item| {
                matching_term_count(
                    &question_terms,
                    &AnswerCandidateTerms::from_evidence(item, &self.context).searchable,
                )
            })
            .max()
            .unwrap_or_default();
        let coverage = best_matches as f64 / concept_count(&question_terms) as f64;
        if coverage >= 0.6 {
            MemoryConfidence::High
        } else if coverage >= 0.3 {
            MemoryConfidence::Medium
        } else {
            MemoryConfidence::Low
        }
    }

    pub(super) fn matched_query_terms(
        &self,
        question: &str,
        evidence: &[MemoryEvidence],
    ) -> Vec<String> {
        let evidence_terms = evidence
            .iter()
            .flat_map(|item| {
                AnswerCandidateTerms::from_evidence(item, &self.context)
                    .searchable
                    .into_iter()
            })
            .collect::<BTreeSet<_>>();
        informative_terms(question)
            .into_iter()
            .filter(|question_term| {
                evidence_terms
                    .iter()
                    .any(|evidence_term| terms_match(question_term, evidence_term))
            })
            .collect()
    }

    pub(super) fn matched_relations(
        &self,
        question: &str,
        evidence: &[MemoryEvidence],
    ) -> Vec<String> {
        let question_terms = informative_terms(question);
        evidence
            .iter()
            .flat_map(|item| self.context.relationships_for(item))
            .filter(|relationship| relationship.matches_any(&question_terms))
            .map(|relationship| relationship.rel.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RelationDirection {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RelationFeature {
    rel: String,
    semantic_class: RelationSemanticClass,
    /// What the writer's own judgment of this edge is worth to retrieval.
    signal: u32,
    direction: RelationDirection,
    other_endpoint_ref: String,
    endpoint_terms: BTreeSet<String>,
    why_terms: BTreeSet<String>,
    evidence_terms: BTreeSet<String>,
    relation_terms: BTreeSet<String>,
}

impl RelationFeature {
    fn searchable_terms(&self) -> BTreeSet<String> {
        self.endpoint_terms
            .iter()
            .chain(&self.why_terms)
            .chain(&self.evidence_terms)
            .chain(&self.relation_terms)
            .cloned()
            .collect()
    }

    fn matches_any(&self, question_terms: &BTreeSet<String>) -> bool {
        let relation_terms = self.searchable_terms();
        question_terms.iter().any(|question_term| {
            relation_terms
                .iter()
                .any(|relation_term| terms_match(question_term, relation_term))
        })
    }

    fn stable_cmp(&self, other: &Self) -> Ordering {
        // Signal first, so the sixteen features a candidate keeps are the
        // best-proven ones rather than whichever class happened to sort low.
        other
            .signal
            .cmp(&self.signal)
            .then_with(|| {
                self.semantic_class
                    .salience_rank()
                    .cmp(&other.semantic_class.salience_rank())
            })
            .then_with(|| self.rel.cmp(&other.rel))
            .then_with(|| self.direction.cmp(&other.direction))
            .then_with(|| self.other_endpoint_ref.cmp(&other.other_endpoint_ref))
            .then_with(|| self.endpoint_terms.cmp(&other.endpoint_terms))
            .then_with(|| self.why_terms.cmp(&other.why_terms))
            .then_with(|| self.evidence_terms.cmp(&other.evidence_terms))
            .then_with(|| self.relation_terms.cmp(&other.relation_terms))
    }
}

#[derive(Default)]
struct AnswerRecallContext {
    details_by_ref: BTreeMap<String, BTreeSet<String>>,
    relationships_by_ref: BTreeMap<String, Vec<RelationFeature>>,
    lifecycle: MemoryLifecycle,
    reach_graph: ReachGraph,
}

impl AnswerRecallContext {
    fn from_bundle(bundle: &KmpBundle) -> Self {
        let details_by_ref = bundle
            .node_details()
            .iter()
            .map(|detail| {
                (
                    detail.node_id().to_string(),
                    informative_terms(detail.detail()),
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
            let endpoint_terms = informative_terms(&format!(
                "{} {}",
                relationship.source_node_id(),
                relationship.target_node_id()
            ));
            let relation_evidence = explanation.evidence().unwrap_or_default();
            let evidence_terms = informative_terms(relation_evidence);
            // A rationale can improve ranking only when the relation carries
            // its own evidence. It remains context, never a freestanding fact.
            let why_terms = if relation_evidence.trim().is_empty() {
                BTreeSet::new()
            } else {
                informative_terms(&format!(
                    "{} {}",
                    explanation.rationale().unwrap_or_default(),
                    explanation.motivation().unwrap_or_default()
                ))
            };
            let relation_terms = informative_terms(relationship.relationship_type());
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
            lifecycle: MemoryLifecycle::read(bundle),
            reach_graph: ReachGraph::from_bundle(bundle),
        }
    }

    fn relationships_for<'a>(&'a self, item: &MemoryEvidence) -> Vec<&'a RelationFeature> {
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

    fn temporal_state(&self, item: &MemoryEvidence) -> CandidateTemporalState {
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
    fn recency_rank(&self, item: &MemoryEvidence) -> u32 {
        self.lifecycle.recency_rank(item.time.as_ref())
    }
}

fn relationship_is_explanatory(relationship: &kmp_domain::BundleRelationship) -> bool {
    match relationship.explanation().semantic_class() {
        RelationSemanticClass::Causal
        | RelationSemanticClass::Motivational
        | RelationSemanticClass::Constraint => true,
        RelationSemanticClass::Evidential => relationship.relationship_type() != "supports",
        RelationSemanticClass::Structural | RelationSemanticClass::Procedural => false,
    }
}

/// Ordered from the strongest evidence of relevance to the weakest.
///
/// Text still leads: nothing below `claim_matches` can lift a candidate over
/// one that answers the question in its own words. What the typed fields buy
/// is everything underneath — where the old key fell straight through to an
/// alphabetical tie-break on the ref.
/// Everything BM25 needs about one question and the candidates it is being
/// asked against.
struct Lexicon {
    question: TermCounts,
    content: LexicalField,
    direct: LexicalField,
    floor: f64,
}

impl Lexicon {
    fn build(question: &str, prepared: &[(MemoryEvidence, AnswerCandidateTerms)]) -> Self {
        let question = informative_term_counts(question);
        let content = LexicalField::build(prepared.iter().map(|(_, terms)| &terms.content_counts));
        let direct = LexicalField::build(prepared.iter().map(|(_, terms)| &terms.direct_counts));
        let floor = direct.eligibility_floor(&question);
        Self {
            question,
            content,
            direct,
            floor,
        }
    }

    /// Whether a candidate says enough about the question to be answering it.
    ///
    /// Measured on the raw score, not the quantized one, so a candidate is
    /// never refused by a rounding boundary.
    fn clears_floor(&self, terms: &AnswerCandidateTerms) -> bool {
        let score = self.direct.score(&self.question, &terms.direct_counts);
        // Sharing nothing at all is its own answer, and no floor derived from
        // an empty overlap should be able to admit it.
        score > 0.0 && score >= self.floor
    }

    fn content_score(&self, terms: &AnswerCandidateTerms) -> i64 {
        ranked_score(self.content.score(&self.question, &terms.content_counts))
    }

    fn direct_score(&self, terms: &AnswerCandidateTerms) -> i64 {
        ranked_score(self.direct.score(&self.question, &terms.direct_counts))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RelevanceKey {
    content_focus_matches: usize,
    /// BM25 of the question against the candidate's own text, in tenths of a
    /// point. It replaced a count of distinct shared concepts, which weighed
    /// a rare word exactly as much as a word every candidate uses.
    content_score: i64,
    /// The same, over the text plus the identifiers and metadata a candidate
    /// is addressed by.
    direct_score: i64,
    claim_matches: usize,
    /// Relations of the kind the question asked for: a *why* question met by
    /// a causal edge, a replacement question met by `supersedes`.
    intent_relation_matches: usize,
    relation_why_matches: usize,
    relation_matches: usize,
    /// The writer's own judgment of the matching relations, summed.
    relation_signal: u32,
    total_matches: usize,
    recency_rank: u32,
}

struct AnswerCandidate {
    relevance: RelevanceKey,
    searchable_terms: BTreeSet<String>,
    stable_key: String,
    item: MemoryEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateTemporalState {
    CurrentOrUnspecified,
    Superseded,
    /// Applicability ended without a replacement. `valid_until` is the only
    /// lifecycle KMP models that names no successor, so it cannot be found
    /// by following `supersedes`.
    Expired,
}

impl AnswerCandidate {
    /// Returns the candidate when the question reaches it directly, and hands
    /// the item back untouched when it does not, so a later pass can still
    /// rescue it through the graph.
    fn eligible(
        item: MemoryEvidence,
        terms: AnswerCandidateTerms,
        question_terms: &BTreeSet<String>,
        strict_focus: Option<&(BTreeSet<String>, usize)>,
        lexicon: &Lexicon,
        intent: &QuestionIntent,
        context: &AnswerRecallContext,
    ) -> Result<Self, Box<MemoryEvidence>> {
        if !lexicon.clears_floor(&terms) {
            return Err(Box::new(item));
        }
        // Both lifecycles end a claim's standing as current advice, and a
        // question that asks about history is asking for exactly them.
        if matches!(
            context.temporal_state(&item),
            CandidateTemporalState::Superseded | CandidateTemporalState::Expired
        ) && !query_requests_lifecycle(question_terms)
        {
            return Err(Box::new(item));
        }

        let answers_requested_focus =
            strict_focus.is_none_or(|(focus_terms, required_focus_matches)| {
                matching_term_count(focus_terms, &terms.searchable) >= *required_focus_matches
            });
        if !answers_requested_focus {
            return Err(Box::new(item));
        }

        let relations = context.relationships_for(&item);
        let relevance = RelevanceKey {
            content_focus_matches: strict_focus
                .map(|(focus_terms, _)| matching_term_count(focus_terms, &terms.content))
                .unwrap_or_default(),
            content_score: lexicon.content_score(&terms),
            direct_score: lexicon.direct_score(&terms),
            claim_matches: matching_term_count(question_terms, &terms.claim),
            intent_relation_matches: intent_relation_matches(intent, &relations),
            relation_why_matches: matching_term_count(question_terms, &terms.relation_why),
            relation_matches: matching_term_count(question_terms, &terms.relation),
            relation_signal: relation_signal_total(question_terms, &relations),
            total_matches: matching_term_count(question_terms, &terms.searchable),
            recency_rank: context.recency_rank(&item),
        };
        Ok(Self {
            relevance,
            searchable_terms: terms.searchable,
            stable_key: stable_evidence_key(&item),
            item,
        })
    }
}

struct AnswerCandidateTerms {
    content: BTreeSet<String>,
    content_counts: TermCounts,
    direct_counts: TermCounts,
    claim: BTreeSet<String>,
    relation_why: BTreeSet<String>,
    relation: BTreeSet<String>,
    searchable: BTreeSet<String>,
}

impl AnswerCandidateTerms {
    fn from_evidence(item: &MemoryEvidence, context: &AnswerRecallContext) -> Self {
        let content = informative_terms(&item.text);
        let content_counts = informative_term_counts(&item.text);
        let mut direct_text = format!("{} {}", item.text, item.source);
        direct_text.push(' ');
        direct_text.push_str(&item.id);
        for supported_ref in &item.supports {
            direct_text.push(' ');
            direct_text.push_str(supported_ref);
        }
        for (key, value) in &item.metadata {
            direct_text.push(' ');
            direct_text.push_str(key);
            direct_text.push(' ');
            direct_text.push_str(value);
        }
        let direct_counts = informative_term_counts(&direct_text);
        let direct = informative_terms(&direct_text);

        let mut claim = item
            .supports
            .iter()
            .flat_map(|supported_ref| informative_terms(supported_ref))
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

fn diversify_candidates(
    question_terms: &BTreeSet<String>,
    focus_terms: &BTreeSet<String>,
    mut candidates: Vec<AnswerCandidate>,
) -> Vec<AnswerCandidate> {
    if candidates.len() < 2 {
        return candidates;
    }

    // Novelty is useful only near the answer boundary. Bounding the greedy
    // window keeps reranking independent of graph degree; the complete proof
    // tail retains its already deterministic relevance order.
    let tail = if candidates.len() > MAX_RERANK_CANDIDATES {
        candidates.split_off(MAX_RERANK_CANDIDATES)
    } else {
        Vec::new()
    };

    let first = candidates.remove(0);
    let mut covered = matching_terms(question_terms, &first.searchable_terms);
    let mut covered_focus = matching_terms(focus_terms, &first.searchable_terms);
    let mut ranked = vec![first];

    while !candidates.is_empty() {
        let mut best_index = 0;
        let mut best_key = None;
        for (index, candidate) in candidates.iter().enumerate() {
            let focus_gain = matching_terms(focus_terms, &candidate.searchable_terms)
                .difference(&covered_focus)
                .count();
            let total_gain = matching_terms(question_terms, &candidate.searchable_terms)
                .difference(&covered)
                .count();
            let key = (candidate.relevance, focus_gain, total_gain);
            if best_key.is_none_or(|current| key > current) {
                best_index = index;
                best_key = Some(key);
            }
        }

        let selected = candidates.remove(best_index);
        covered.extend(matching_terms(question_terms, &selected.searchable_terms));
        covered_focus.extend(matching_terms(focus_terms, &selected.searchable_terms));
        ranked.push(selected);
    }
    ranked.extend(tail);
    ranked
}

fn prioritize_distinct_claims(candidates: Vec<AnswerCandidate>) -> Vec<AnswerCandidate> {
    let mut seen_claims = BTreeSet::new();
    let mut distinct = Vec::with_capacity(candidates.len());
    let mut repeated = Vec::new();

    for candidate in candidates {
        let claim = candidate
            .item
            .supports
            .first()
            .map(String::as_str)
            .unwrap_or(candidate.item.source.as_str());
        if seen_claims.insert(claim.to_string()) {
            distinct.push(candidate);
        } else {
            repeated.push(candidate);
        }
    }
    distinct.extend(repeated);
    distinct
}

fn stable_evidence_key(item: &MemoryEvidence) -> String {
    format!(
        "{}\u{0}{}\u{0}{}\u{0}{}",
        item.id,
        item.supports
            .first()
            .map(String::as_str)
            .unwrap_or_default(),
        item.source,
        item.text
    )
}

fn answer_context_refs(item: &MemoryEvidence) -> BTreeSet<String> {
    item.id
        .strip_prefix("detail:")
        .map(str::to_string)
        .into_iter()
        .chain(item.supports.iter().cloned())
        .collect()
}

/// How many of a candidate's relations are the kind the question asked for.
///
/// An unspecific question asks for none, and every candidate scores zero —
/// which is what the ranker did before questions had an intent at all.
fn intent_relation_matches(intent: &QuestionIntent, relations: &[&RelationFeature]) -> usize {
    if intent.is_unspecific() {
        return 0;
    }
    relations
        .iter()
        .filter(|relation| intent.matches(&relation.rel, &relation.semantic_class))
        .count()
}

/// The writer's judgment of the relations that actually touch the question,
/// summed and capped.
///
/// Only matching relations count: a candidate cannot climb by being densely
/// connected to material the question never mentioned.
fn relation_signal_total(question_terms: &BTreeSet<String>, relations: &[&RelationFeature]) -> u32 {
    relations
        .iter()
        .filter(|relation| relation.matches_any(question_terms))
        .map(|relation| relation.signal)
        .sum()
}

/// Records how a rescued candidate was reached, so the hop can be audited
/// rather than trusted.
fn mark_reached(mut item: MemoryEvidence, hop: &RelationReach) -> MemoryEvidence {
    item.metadata
        .insert(REACHED_BY_KEY.to_string(), REACHED_BY_RELATION.to_string());
    item.metadata
        .insert("reached_from".to_string(), hop.from_ref.clone());
    item.metadata
        .insert("reached_via".to_string(), hop.via_relation.clone());
    item.metadata
        .insert("reached_hops".to_string(), hop.hops.to_string());
    item
}

/// Whether a candidate arrived through the graph rather than through the
/// question's own words.
pub(super) fn was_reached_by_relation(item: &MemoryEvidence) -> bool {
    item.metadata
        .get(REACHED_BY_KEY)
        .is_some_and(|reach| reach == REACHED_BY_RELATION)
}

fn matching_terms(
    question_terms: &BTreeSet<String>,
    evidence_terms: &BTreeSet<String>,
) -> BTreeSet<String> {
    question_terms
        .iter()
        .filter(|question_term| {
            evidence_terms
                .iter()
                .any(|evidence_term| terms_match(question_term, evidence_term))
        })
        .cloned()
        .collect()
}

fn matching_term_count(
    question_terms: &BTreeSet<String>,
    evidence_terms: &BTreeSet<String>,
) -> usize {
    matching_terms(question_terms, evidence_terms)
        .iter()
        .map(|term| concept_key(term))
        .collect::<BTreeSet<_>>()
        .len()
}

fn concept_count(terms: &BTreeSet<String>) -> usize {
    terms
        .iter()
        .map(|term| concept_key(term))
        .collect::<BTreeSet<_>>()
        .len()
}

/// Extracts the subject-bearing clause used by strict answer policies.
fn strict_answer_focus_terms(question: &str) -> BTreeSet<String> {
    const CONTEXT_BOUNDARIES: &[&str] = &[
        "after", "before", "because", "if", "once", "when", "while", "antes", "cuando", "despues",
        "después", "mientras", "porque", "si",
    ];
    const GENERIC_QUESTION_PREDICATES: &[&str] = &[
        "happen", "happened", "occur", "occurred", "ocurrio", "ocurrió", "paso", "pasó", "prove",
        "proved", "proves",
    ];

    let main_clause = question
        .split(|character: char| !character.is_alphanumeric())
        .map(fold_search_term)
        .take_while(|token| !CONTEXT_BOUNDARIES.contains(&token.as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let mut terms = informative_terms(&main_clause);
    for predicate in GENERIC_QUESTION_PREDICATES {
        terms.remove(*predicate);
    }
    if terms.is_empty() {
        informative_terms(question)
    } else {
        terms
    }
}

/// The same tokens `informative_terms` yields, kept with their frequencies
/// and collapsed onto concept keys so BM25 weighs a synonym pair once.
fn informative_term_counts(value: &str) -> TermCounts {
    informative_tokens(value)
        .map(|term| concept_key(&term).to_string())
        .collect()
}

fn informative_terms(value: &str) -> BTreeSet<String> {
    informative_tokens(value).collect()
}

fn informative_tokens(value: &str) -> impl Iterator<Item = String> + '_ {
    const STOP_WORDS: &[&str] = &[
        "a", "against", "an", "and", "are", "as", "at", "be", "because", "by", "came", "did", "do",
        "does", "earlier", "for", "from", "he", "how", "i", "if", "in", "is", "it", "me", "more",
        "my", "of", "on", "one", "or", "plus", "same", "should", "than", "the", "this", "to", "us",
        "use", "used", "uses", "was", "we", "were", "what", "when", "where", "which", "who", "why",
        "will", "with", "el", "la", "los", "las", "de", "al", "del", "donde", "en", "es", "lo",
        "no", "por", "para", "que", "se", "su", "un", "ya", "como", "cual", "cuando",
    ];
    value
        .split(|character: char| !character.is_alphanumeric())
        .map(fold_search_term)
        .filter(|term| {
            !term.is_empty()
                && !STOP_WORDS.contains(&term.as_str())
                && (term.chars().all(|character| character.is_ascii_digit()) || term.len() >= 2)
        })
}

/// Produces the comparison form only. Stored evidence and returned query text
/// stay byte-exact; the ranker indexes this folded sibling so a phone or
/// foreign keyboard does not turn `válvula`, `arrêt`, `refrigeração`,
/// `Straße`, or `Kühlventil` into an unreachable memory.
pub(super) fn fold_search_term(value: &str) -> String {
    let mut folded = String::with_capacity(value.len());
    for character in value
        .nfkd()
        .filter(|character| !is_combining_mark(*character))
    {
        match character {
            'ß' | 'ẞ' => folded.push_str("ss"),
            _ => folded.extend(character.to_lowercase()),
        }
    }
    folded
}

fn terms_match(left: &str, right: &str) -> bool {
    concept_key(left) == concept_key(right)
}

/// Stable semantic buckets for the small set of paraphrases the deterministic
/// ranker intentionally understands. Counting buckets rather than raw words
/// prevents a question containing two synonyms from earning two matches from
/// one evidence term.
fn concept_key(term: &str) -> &str {
    match term {
        "query" | "recall" | "retrieval" | "retrieve" => "concept:recall",
        "accept" | "accepted" | "acceptance" => "concept:acceptance",
        "correct" | "corrected" | "correction" | "fix" | "fixed" => "concept:correction",
        "remain" | "remains" | "remaining" | "still" => "concept:currentness",
        "destination" | "move" | "moved" | "moves" | "moving" | "relocate" | "relocated"
        | "relocates" | "relocating" => "concept:movement",
        "replace" | "replaced" | "replaces" | "replacing" | "supersede" | "superseded"
        | "supersedes" => "concept:lifecycle",
        "backend" | "engine" | "sqlite" => "concept:storage-engine",
        "data" | "directory" | "store" | "stores" | "storage" => "concept:store",
        "build" | "builds" | "built" | "create" | "created" | "fresh" | "install"
        | "installation" | "installed" | "new" | "reinstall" | "reinstalled" => {
            "concept:installation"
        }
        "restart" | "restarted" | "restarting" => "concept:restart",
        "require" | "required" | "requires" => "concept:requirement",
        "check" | "checked" | "validate" | "validated" | "validation" => "concept:validation",
        "rank" | "ranked" | "ranking" | "relevance" => "concept:ranking",
        "old" | "older" | "previous" | "prior" | "stale" => "concept:historical",
        "default" | "select" | "selected" | "selection" => "concept:selection",
        "existing" | "present" | "preserve" | "preserved" | "preserving" => "concept:presence",
        _ => term,
    }
}

fn query_requests_lifecycle(question_terms: &BTreeSet<String>) -> bool {
    const LIFECYCLE_QUERY_TERMS: &[&str] = &[
        "before",
        "former",
        "old",
        "previous",
        "replace",
        "replaced",
        "replaces",
        "replacing",
        "supersede",
        "superseded",
        "supersedes",
    ];
    question_terms
        .iter()
        .any(|term| LIFECYCLE_QUERY_TERMS.contains(&term.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kmp_domain::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, RelationExplanation, Role,
    };
    use prost_types::Timestamp;

    fn ev(source: &str) -> MemoryEvidence {
        MemoryEvidence {
            id: format!("detail:{source}"),
            supports: vec![source.to_string()],
            text: source.to_string(),
            source: source.to_string(),
            time: None,
            metadata: Default::default(),
        }
    }

    fn claim_ev(id: &str, claim: &str, text: &str) -> MemoryEvidence {
        MemoryEvidence {
            id: format!("detail:{id}"),
            supports: vec![claim.to_string()],
            text: text.to_string(),
            source: "fixture".to_string(),
            time: None,
            metadata: Default::default(),
        }
    }

    fn relation(rel: &str, class: RelationSemanticClass, why: &str) -> RelationFeature {
        RelationFeature {
            rel: rel.to_string(),
            semantic_class: class,
            signal: 0,
            direction: RelationDirection::Outgoing,
            other_endpoint_ref: "target".to_string(),
            endpoint_terms: informative_terms("source target"),
            why_terms: informative_terms(why),
            evidence_terms: BTreeSet::new(),
            relation_terms: informative_terms(rel),
        }
    }

    fn ranker_with_claim_relation(
        claim: &str,
        rel: &str,
        class: RelationSemanticClass,
        why: &str,
    ) -> AnswerEvidenceRanker {
        AnswerEvidenceRanker {
            context: AnswerRecallContext {
                details_by_ref: BTreeMap::new(),
                relationships_by_ref: BTreeMap::from([(
                    claim.to_string(),
                    vec![relation(rel, class, why)],
                )]),
                ..Default::default()
            },
        }
    }

    fn ranker_from_relationship(
        rel: &str,
        class: RelationSemanticClass,
        why: &str,
    ) -> AnswerEvidenceRanker {
        ranker_from_relationship_with_evidence(
            rel,
            class,
            why,
            Some("Captured in the architecture decision record."),
        )
    }

    fn ranker_from_relationship_with_evidence(
        rel: &str,
        class: RelationSemanticClass,
        why: &str,
        relation_evidence: Option<&str>,
    ) -> AnswerEvidenceRanker {
        let node = |id: &str| {
            BundleNode::new(
                id,
                "memory",
                id,
                "fixture",
                "ACTIVE",
                Vec::new(),
                BTreeMap::new(),
            )
        };
        let mut explanation = RelationExplanation::new(class).with_rationale(why);
        if let Some(relation_evidence) = relation_evidence {
            explanation = explanation.with_evidence(relation_evidence);
        }
        let relationship = BundleRelationship::new("claim:adr", "claim:current", rel, explanation);
        let bundle = KmpBundle::new(
            CaseId::new("claim:adr").expect("valid case id"),
            Role::new("developer").expect("valid role"),
            node("claim:adr"),
            vec![node("claim:current")],
            vec![relationship],
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("valid bundle");
        AnswerEvidenceRanker::from_bundle(&bundle)
    }

    fn proven(class: RelationSemanticClass) -> RelationExplanation {
        RelationExplanation::new(class)
            .with_rationale("the reserve was diverted before the repair")
            .with_evidence("recorded in the incident review")
            .with_confidence("high")
    }

    fn timed_entry(scope: &str, entry: &str, observed_at: &str) -> BundleRelationship {
        entry_relationship(scope, entry, observed_at, None)
    }

    fn expiring_entry(
        scope: &str,
        entry: &str,
        observed_at: &str,
        valid_until: &str,
    ) -> BundleRelationship {
        entry_relationship(scope, entry, observed_at, Some(valid_until))
    }

    fn entry_relationship(
        scope: &str,
        entry: &str,
        observed_at: &str,
        valid_until: Option<&str>,
    ) -> BundleRelationship {
        let mut explanation = RelationExplanation::new(RelationSemanticClass::Structural)
            .with_dimension("timeline")
            .with_scope_id(scope)
            .with_observed_at(observed_at);
        if let Some(valid_until) = valid_until {
            explanation = explanation.with_valid_until(valid_until);
        }
        BundleRelationship::new(scope, entry, "contains_entry", explanation)
    }

    fn ranker_over(relationships: Vec<BundleRelationship>, refs: &[&str]) -> AnswerEvidenceRanker {
        let node = |id: &str| {
            BundleNode::new(
                id,
                "memory",
                id,
                "fixture",
                "ACTIVE",
                Vec::new(),
                BTreeMap::new(),
            )
        };
        let bundle = KmpBundle::new(
            CaseId::new("claim:root").expect("valid case id"),
            Role::new("answerer").expect("valid role"),
            node("claim:root"),
            refs.iter().map(|id| node(id)).collect(),
            relationships,
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("valid bundle");
        AnswerEvidenceRanker::from_bundle(&bundle)
    }

    /// Five candidates repeating the same common words, and one that shares a
    /// single rare one. The rule this replaces demanded two shared concepts
    /// from a question this long and discarded the rare match unscored.
    #[test]
    fn a_single_rare_match_is_no_longer_discarded_for_being_alone() {
        let mut candidates = (0..5)
            .map(|index| {
                claim_ev(
                    &format!("common-{index}"),
                    &format!("claim:common{index}"),
                    "The shared store rollout was discussed again.",
                )
            })
            .collect::<Vec<_>>();
        candidates.push(claim_ev(
            "rare",
            "claim:rare",
            "Valkey was benchmarked overnight.",
        ));

        let ranked = AnswerEvidenceRanker::default().rank(
            "Which valkey adapter handles the shared store rollout?",
            MemoryAnswerPolicy::BestEffort,
            candidates,
        );

        assert_eq!(ranked[0].id, "detail:rare");
    }

    /// Counting shared concepts said two common words beat one rare one.
    /// Weighing them says the opposite, which is the whole point of an IDF.
    #[test]
    fn one_rare_match_outranks_two_common_ones() {
        let mut candidates = (0..4)
            .map(|index| {
                claim_ev(
                    &format!("filler-{index}"),
                    &format!("claim:filler{index}"),
                    "The shared store was mentioned.",
                )
            })
            .collect::<Vec<_>>();
        candidates.push(claim_ev(
            "common-pair",
            "claim:pair",
            "The shared store was mentioned again.",
        ));
        candidates.push(claim_ev("rare-one", "claim:rare", "Valkey was chosen."));

        let ranked = AnswerEvidenceRanker::default().rank(
            "Was valkey chosen for the shared store?",
            MemoryAnswerPolicy::BestEffort,
            candidates,
        );

        assert_eq!(ranked[0].id, "detail:rare-one");
    }

    /// Two candidates say the same rare thing; one says a great deal else.
    /// Length used to be free surface to be hit on, and the tie fell to the
    /// alphabetical order of the ref.
    #[test]
    fn a_longer_candidate_does_not_win_for_having_more_surface() {
        let ranked = AnswerEvidenceRanker::default().rank(
            "Was valkey chosen?",
            MemoryAnswerPolicy::BestEffort,
            vec![
                claim_ev(
                    "a-padded",
                    "claim:padded",
                    "Valkey was chosen, and the standup moved, and the calendar invite changed, \
                     and the agenda was rewritten, and the room was rebooked for the week.",
                ),
                claim_ev("z-short", "claim:short", "Valkey was chosen."),
            ],
        );

        assert_eq!(ranked[0].id, "detail:z-short");
    }

    #[test]
    fn a_proven_relation_reaches_the_cause_that_shares_no_words_with_the_question() {
        let ranker = ranker_over(
            vec![BundleRelationship::new(
                "claim:outage",
                "claim:cause",
                "triggers",
                proven(RelationSemanticClass::Causal),
            )],
            &["claim:outage", "claim:cause"],
        );

        let ranked = ranker.rank(
            "Why did the checkout outage happen?",
            MemoryAnswerPolicy::BestEffort,
            vec![
                claim_ev("outage", "claim:outage", "The checkout outage happened."),
                claim_ev(
                    "cause",
                    "claim:cause",
                    "Reserve power was diverted for a valve repair.",
                ),
            ],
        );

        assert_eq!(ranked[0].id, "detail:outage");
        assert_eq!(ranked[1].id, "detail:cause");
        assert!(was_reached_by_relation(&ranked[1]));
        assert_eq!(ranked[1].metadata["reached_from"], "claim:outage");
        assert_eq!(ranked[1].metadata["reached_via"], "triggers");
        assert_eq!(ranked[1].metadata["reached_hops"], "1");
        assert!(!was_reached_by_relation(&ranked[0]));
    }

    #[test]
    fn an_unproven_relation_still_reaches_nothing() {
        let ranker = ranker_over(
            vec![BundleRelationship::new(
                "claim:outage",
                "claim:cause",
                "follows",
                proven(RelationSemanticClass::Procedural),
            )],
            &["claim:outage", "claim:cause"],
        );

        let ranked = ranker.rank(
            "Why did the checkout outage happen?",
            MemoryAnswerPolicy::BestEffort,
            vec![
                claim_ev("outage", "claim:outage", "The checkout outage happened."),
                claim_ev(
                    "cause",
                    "claim:cause",
                    "Reserve power was diverted for a valve repair.",
                ),
            ],
        );

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].id, "detail:outage");
    }

    #[test]
    fn an_entry_whose_applicability_ended_is_not_offered_as_current() {
        let ranker = ranker_over(
            vec![
                expiring_entry(
                    "scope:timeline",
                    "claim:expired",
                    "2026-01-01T00:00:00Z",
                    "2026-02-01T00:00:00Z",
                ),
                timed_entry("scope:timeline", "claim:live", "2026-03-01T00:00:00Z"),
            ],
            &["scope:timeline", "claim:expired", "claim:live"],
        );

        let current = ranker.rank(
            "Which release window applies to the shared store?",
            MemoryAnswerPolicy::BestEffort,
            vec![
                claim_ev(
                    "expired",
                    "claim:expired",
                    "The release window applies to the shared store on Mondays.",
                ),
                claim_ev(
                    "live",
                    "claim:live",
                    "The release window applies to the shared store on Fridays.",
                ),
            ],
        );

        assert_eq!(current.len(), 1);
        assert_eq!(current[0].id, "detail:live");
    }

    #[test]
    fn a_lifecycle_question_can_still_audit_what_expired() {
        let ranker = ranker_over(
            vec![
                expiring_entry(
                    "scope:timeline",
                    "claim:expired",
                    "2026-01-01T00:00:00Z",
                    "2026-02-01T00:00:00Z",
                ),
                timed_entry("scope:timeline", "claim:live", "2026-03-01T00:00:00Z"),
            ],
            &["scope:timeline", "claim:expired", "claim:live"],
        );

        let audited = ranker.rank(
            "Which release window was the previous one for the shared store?",
            MemoryAnswerPolicy::BestEffort,
            vec![claim_ev(
                "expired",
                "claim:expired",
                "The release window applies to the shared store on Mondays.",
            )],
        );

        assert_eq!(audited.len(), 1);
        assert_eq!(audited[0].id, "detail:expired");
    }

    #[test]
    fn the_relation_the_question_asked_for_outranks_an_otherwise_equal_candidate() {
        let ranker = ranker_over(
            vec![
                BundleRelationship::new(
                    "claim:motivated",
                    "claim:decision",
                    "chosen_because",
                    proven(RelationSemanticClass::Motivational),
                ),
                BundleRelationship::new(
                    "claim:catalogued",
                    "claim:decision",
                    "component_of",
                    proven(RelationSemanticClass::Structural),
                ),
            ],
            &["claim:motivated", "claim:catalogued", "claim:decision"],
        );

        let ranked = ranker.rank(
            "Why was the shared engine selected?",
            MemoryAnswerPolicy::BestEffort,
            vec![
                claim_ev(
                    "catalogued",
                    "claim:catalogued",
                    "The shared engine was selected.",
                ),
                claim_ev(
                    "motivated",
                    "claim:motivated",
                    "The shared engine was selected.",
                ),
            ],
        );

        assert_eq!(ranked[0].id, "detail:motivated");
    }

    #[test]
    fn recency_breaks_a_tie_that_used_to_fall_to_the_reference_name() {
        let ranker = ranker_over(
            vec![timed_entry(
                "scope:timeline",
                "claim:a",
                "2026-03-01T00:00:00Z",
            )],
            &["scope:timeline", "claim:a"],
        );
        let older = MemoryEvidence {
            time: Some(Timestamp {
                seconds: 1_735_689_600,
                nanos: 0,
            }),
            ..claim_ev("a-older", "claim:older", "the shared engine is documented")
        };
        let newer = MemoryEvidence {
            time: Some(Timestamp {
                seconds: 1_772_323_200,
                nanos: 0,
            }),
            ..claim_ev("z-newer", "claim:newer", "the shared engine is documented")
        };

        let ranked = ranker.rank(
            "Which shared engine is documented?",
            MemoryAnswerPolicy::BestEffort,
            vec![older, newer],
        );

        assert_eq!(ranked[0].id, "detail:z-newer");
    }

    #[test]
    fn exact_late_match_leads_more_than_three_hundred_eligible_items() {
        let question =
            "What exact deficiencies caused rejection and what authority remains withheld?";
        let mut candidates = (0..301)
            .map(|index| {
                ev(&format!(
                    "Gate deficiencies caused rejection; authority remains withheld for unrelated rollout {index}."
                ))
            })
            .collect::<Vec<_>>();
        let exact = "The exact deficiencies caused rejection were missing contract tests; authority remains withheld until the gate passes.";
        candidates.push(ev(exact));

        let ranked = AnswerEvidenceRanker::default().rank(
            question,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            candidates,
        );

        assert_eq!(ranked[0].text, exact);
        assert_eq!(
            AnswerEvidenceRanker::default().confidence(question, &ranked[..5]),
            MemoryConfidence::High
        );
    }

    #[test]
    fn graph_why_relevance_survives_diversification() {
        let question = "What sqlite storage engine replaced the previous one for shared concurrent processes migration?";
        let candidates = vec![
            claim_ev(
                "first",
                "claim:first",
                "sqlite storage engine shared migration",
            ),
            claim_ev(
                "graph-answer",
                "claim:graph",
                "sqlite storage engine shared",
            ),
            claim_ev(
                "weak-replaced",
                "claim:weak",
                "sqlite storage engine replaced",
            ),
        ];
        let context = AnswerRecallContext {
            details_by_ref: BTreeMap::new(),
            relationships_by_ref: BTreeMap::from([
                (
                    "claim:first".to_string(),
                    vec![relation(
                        "chosen_because",
                        RelationSemanticClass::Motivational,
                        "sqlite storage engine replaced previous shared concurrent processes migration",
                    )],
                ),
                (
                    "claim:graph".to_string(),
                    vec![relation(
                        "chosen_because",
                        RelationSemanticClass::Motivational,
                        "sqlite storage engine replaced previous shared concurrent processes migration",
                    )],
                ),
                (
                    "claim:weak".to_string(),
                    vec![RelationFeature {
                        why_terms: informative_terms("shared migration"),
                        ..relation("chosen_because", RelationSemanticClass::Motivational, "")
                    }],
                ),
            ]),
            ..Default::default()
        };
        let ranker = AnswerEvidenceRanker { context };

        let ranked = ranker.rank(question, MemoryAnswerPolicy::EvidenceOrUnknown, candidates);

        assert_eq!(ranked[0].id, "detail:first");
        assert_eq!(ranked[1].id, "detail:graph-answer");
        assert!(
            ranker
                .matched_relations(question, &ranked[..2])
                .contains(&"chosen_because".to_string())
        );
    }

    #[test]
    fn candidate_permutations_produce_the_same_order() {
        let question = "Which sqlite engine replaced the previous one for concurrent processes?";
        let candidates = vec![
            claim_ev("c", "claim:c", "sqlite engine previous concurrent"),
            claim_ev("a", "claim:a", "sqlite engine replaced processes"),
            claim_ev("b", "claim:b", "sqlite engine replaced concurrent"),
        ];
        let ranker = AnswerEvidenceRanker::default();
        let forward = ranker.rank(
            question,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            candidates.clone(),
        );
        let reverse = ranker.rank(
            question,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            candidates.into_iter().rev().collect(),
        );

        assert_eq!(
            forward.iter().map(|item| &item.id).collect::<Vec<_>>(),
            reverse.iter().map(|item| &item.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn weak_distractor_terms_do_not_displace_subject_bearing_evidence() {
        let question =
            "Which embedded storage engine replaced the previous one for two KMP processes?";
        let answer = claim_ev(
            "answer",
            "claim:answer",
            "SQLite replaced the previous embedded storage engine for two KMP processes.",
        );
        let baseline = AnswerEvidenceRanker::default().rank(
            question,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![answer.clone()],
        );
        let with_distractors = AnswerEvidenceRanker::default().rank(
            question,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![
                claim_ev("earlier", "claim:earlier", "KMP earlier used one format"),
                claim_ev("same", "claim:same", "KMP has more than one same feature"),
                answer,
            ],
        );

        assert_eq!(baseline[0].id, "detail:answer");
        assert_eq!(with_distractors[0].id, "detail:answer");
    }

    #[test]
    fn graph_only_match_cannot_cross_the_direct_eligibility_floor() {
        let ranker = ranker_with_claim_relation(
            "claim:unrelated",
            "chosen_because",
            RelationSemanticClass::Motivational,
            "sqlite replaced the previous engine for concurrent processes",
        );
        let ranked = ranker.rank(
            "Which sqlite engine replaced the previous one for concurrent processes?",
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![claim_ev(
                "unrelated",
                "claim:unrelated",
                "The release documentation was published.",
            )],
        );

        assert!(ranked.is_empty());
    }

    #[test]
    fn lifecycle_relation_why_can_rerank_an_eligible_candidate() {
        let question = "Which sqlite engine replaced the previous one?";
        let candidate = claim_ev("adr", "claim:adr", "sqlite engine previous migration");
        let distractor = claim_ev("other", "claim:other", "sqlite engine previous rollout");
        let ranker = ranker_with_claim_relation(
            "claim:adr",
            "supersedes",
            RelationSemanticClass::Evidential,
            "SQLite superseded the previous engine during the migration.",
        );

        let ranked = ranker.rank(
            question,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![distractor, candidate],
        );

        assert_eq!(ranked[0].id, "detail:adr");
        assert_eq!(
            ranker.matched_relations(question, &ranked[..1]),
            vec!["supersedes"]
        );
    }

    #[test]
    fn bundle_relation_type_direction_why_and_evidence_feed_the_reranker() {
        let question = "Which sqlite engine replaced the previous one?";
        let ranker = ranker_from_relationship(
            "supersedes",
            RelationSemanticClass::Evidential,
            "SQLite superseded the previous engine during the migration.",
        );
        let ranked = ranker.rank(
            question,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![
                claim_ev("other", "claim:other", "sqlite engine previous migration"),
                claim_ev("adr", "claim:adr", "sqlite engine previous migration"),
            ],
        );

        assert_eq!(ranked[0].id, "detail:adr");
        assert_eq!(
            ranker.matched_relations(question, &ranked[..1]),
            vec!["supersedes"]
        );
        let feature = &ranker.context.relationships_by_ref["claim:adr"][0];
        assert_eq!(feature.direction, RelationDirection::Outgoing);
        assert_eq!(feature.other_endpoint_ref, "claim:current");
        assert!(feature.evidence_terms.contains("architecture"));
    }

    #[test]
    fn structural_bundle_relationships_never_supply_answer_vocabulary() {
        let ranker = ranker_from_relationship(
            "contains_entry",
            RelationSemanticClass::Structural,
            "SQLite replaced the previous engine for concurrent processes.",
        );
        let ranked = ranker.rank(
            "Which sqlite engine replaced the previous one?",
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![claim_ev("weak", "claim:adr", "sqlite engine documentation")],
        );

        assert!(ranked.is_empty());
        assert!(ranker.context.relationships_by_ref.is_empty());
    }

    #[test]
    fn unsupported_relation_why_is_not_used_as_answer_context() {
        let ranker = ranker_from_relationship_with_evidence(
            "chosen_because",
            RelationSemanticClass::Motivational,
            "SQLite replaced the previous engine for concurrent processes.",
            None,
        );
        let ranked = ranker.rank(
            "Which sqlite engine replaced the previous one?",
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![claim_ev("weak", "claim:adr", "sqlite engine documentation")],
        );

        assert!(ranked.is_empty());
        assert!(
            ranker.context.relationships_by_ref["claim:adr"][0]
                .why_terms
                .is_empty()
        );
    }

    #[test]
    fn superseded_claim_is_not_returned_as_current_advice() {
        let ranker = ranker_from_relationship(
            "supersedes",
            RelationSemanticClass::Evidential,
            "SQLite replaced the previous engine for the shared embedded store.",
        );
        let ranked = ranker.rank(
            "Which embedded engine is current for the shared store?",
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![
                claim_ev(
                    "old",
                    "claim:current",
                    "the previous engine is current for the shared embedded store",
                ),
                claim_ev(
                    "new",
                    "claim:adr",
                    "SQLite is the current embedded engine for the shared store",
                ),
            ],
        );

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].id, "detail:new");
    }

    #[test]
    fn lifecycle_question_can_audit_superseded_claim() {
        let ranker = ranker_from_relationship(
            "supersedes",
            RelationSemanticClass::Evidential,
            "SQLite replaced the previous engine for the shared embedded store.",
        );
        let ranked = ranker.rank(
            "Which engine was replaced before SQLite?",
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![claim_ev(
                "old",
                "claim:current",
                "the previous engine came before SQLite",
            )],
        );

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].id, "detail:old");
        assert_eq!(
            ranker.matched_relations("Which engine was replaced?", &ranked),
            vec!["supersedes"]
        );
    }

    #[test]
    fn supports_relationship_is_never_graph_ranking_context() {
        let ranker = ranker_with_claim_relation(
            "claim:popular",
            "supports",
            RelationSemanticClass::Evidential,
            "sqlite replaced the previous engine for concurrent processes",
        );
        let ranked = ranker.rank(
            "Which sqlite engine replaced the previous one for concurrent processes?",
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![claim_ev(
                "weak",
                "claim:popular",
                "sqlite engine documentation",
            )],
        );

        assert!(ranked.is_empty());
    }

    #[test]
    fn distinct_supported_claims_precede_repeated_citations() {
        let ranker = AnswerEvidenceRanker::default();
        let ranked = ranker.rank(
            "Which primary evidence supports the claim?",
            MemoryAnswerPolicy::BestEffort,
            vec![
                claim_ev("a-primary", "claim:a", "primary evidence supports claim"),
                claim_ev("a-secondary", "claim:a", "primary evidence supports claim"),
                claim_ev("b", "claim:b", "primary evidence supports claim"),
                claim_ev("c", "claim:c", "primary evidence supports claim"),
            ],
        );

        assert_eq!(
            ranked
                .iter()
                .map(|item| item.supports[0].as_str())
                .collect::<Vec<_>>(),
            vec!["claim:a", "claim:b", "claim:c", "claim:a"]
        );
    }

    #[test]
    fn strict_policies_reject_context_that_omits_the_requested_subject() {
        let question = "Which database engine was used when the CI workflow concluded and the remote branch remained present?";
        let contextual = ev("The CI workflow concluded and the remote branch remained present.");

        for policy in [
            MemoryAnswerPolicy::EvidenceOrUnknown,
            MemoryAnswerPolicy::ShowConflicts,
        ] {
            let ranked =
                AnswerEvidenceRanker::default().rank(question, policy, vec![contextual.clone()]);
            assert!(
                ranked.is_empty(),
                "strict policy {policy:?} answered from context alone"
            );
        }
    }

    #[test]
    fn confidence_describes_only_retained_evidence() {
        let ranker = AnswerEvidenceRanker::default();
        let question = "Which exact deficiencies caused rejection and authority withheld?";
        let weak = ev("The rejection affected authority in an unrelated rollout.");
        let exact = ev(
            "The exact deficiencies caused rejection, so authority remained withheld until tests passed.",
        );

        assert_eq!(
            ranker.confidence(question, &[weak]),
            MemoryConfidence::Medium
        );
        assert_eq!(
            ranker.confidence(question, &[exact]),
            MemoryConfidence::High
        );
    }

    #[test]
    fn short_identifiers_and_lifecycle_paraphrases_are_preserved() {
        let terms = informative_terms("PR #83 C1 M1 P0 ID 7 is to un");
        for identifier in ["pr", "83", "c1", "m1", "p0", "id", "7"] {
            assert!(
                terms.contains(identifier),
                "missing identifier {identifier}"
            );
        }
        assert!(terms_match("replaced", "supersedes"));
        assert!(terms_match("retrieval", "query"));
        assert!(terms_match("move", "destination"));
        assert!(!terms_match("database", "branch"));
    }

    #[test]
    fn shared_prefixes_do_not_create_semantic_matches() {
        assert!(!terms_match("prefix", "prefer"));
        assert!(!terms_match("deliberate", "delivered"));
    }

    #[test]
    fn diacritic_free_queries_retrieve_supported_languages_without_rewriting_evidence() {
        let ranker = AnswerEvidenceRanker::default();
        for (query, stored) in [
            ("valvula", "La válvula falló por sedimentación."),
            ("refrigeracao", "A refrigeração parou."),
            ("calcificacao", "A calcificação bloqueou a válvula."),
            ("manutencao", "A manutenção terminou."),
            ("arret", "L'arrêt venait du dépôt."),
            ("strasse", "Die Straße blieb gesperrt."),
            ("kuhlventil", "Das Kühlventil wurde ersetzt."),
        ] {
            let ranked = ranker.rank(
                query,
                MemoryAnswerPolicy::EvidenceOrUnknown,
                vec![ev(stored)],
            );
            assert_eq!(ranked.len(), 1, "{query} did not retrieve {stored}");
            assert_eq!(ranked[0].text, stored, "stored evidence was rewritten");
            assert_eq!(
                ranker.matched_query_terms(query, &ranked),
                vec![query.to_string()]
            );
        }

        assert_eq!(informative_terms("VÁLVULA"), informative_terms("valvula"));
        assert_eq!(fold_search_term("Straße"), "strasse");
    }

    #[test]
    fn shared_prefixes_cannot_turn_unrelated_evidence_into_an_answer() {
        let ranked = AnswerEvidenceRanker::default().rank(
            "Was the kernel prefix a deliberate decision?",
            MemoryAnswerPolicy::EvidenceOrUnknown,
            vec![ev(
                "The kernel prefer mode delivered a decision from another workflow.",
            )],
        );

        assert!(ranked.is_empty());
    }

    #[test]
    fn synonyms_form_one_scoring_concept() {
        let question_terms = informative_terms("new installation");
        let evidence_terms = informative_terms("fresh build");

        assert_eq!(concept_count(&question_terms), 1);
        assert_eq!(matching_term_count(&question_terms, &evidence_terms), 1);
    }

    #[test]
    fn partial_default_update_answers_both_current_state_paraphrases() {
        let current = claim_ev(
            "current-default",
            "decision:fresh-store-default",
            "Shipped KMP builds create fresh SQLite stores while preserving existing legacy stores.",
        );
        let historical = claim_ev(
            "historical-default",
            "decision:historical-default",
            "A single-writer layout was the distribution default for a KMP data directory.",
        );
        let ranker = AnswerEvidenceRanker::default();

        for question in [
            "What is the current default storage engine for a fresh KMP data directory?",
            "Which backend will a new installation select when no existing store is present?",
        ] {
            let ranked = ranker.rank(
                question,
                MemoryAnswerPolicy::EvidenceOrUnknown,
                vec![historical.clone(), current.clone()],
            );

            assert_eq!(ranked[0].id, "detail:current-default", "{question}");
        }
    }
}
