use std::collections::BTreeSet;

use kmp_application::MemoryAnswerPolicy;
use kmp_domain::KmpBundle;
use kmp_proto::v1beta1::{MemoryConfidence, MemoryEvidence};

use super::answer_candidate::AnswerCandidate;
use super::answer_candidate_terms::AnswerCandidateTerms;
use super::answer_recall_context::AnswerRecallContext;
use super::answer_selection::{
    REACHED_BY_ASSOCIATION, answer_context_refs, diversify_candidates, mark_reached,
    mark_reached_by, prioritize_distinct_claims, stable_evidence_key,
};
use super::candidate_temporal_state::CandidateTemporalState;
use super::lexicon::Lexicon;
use super::question_intent::QuestionIntent;
use super::search_terms::{
    concept_count, informative_terms, informative_tokens, matching_term_count, search_key,
    strict_answer_focus_terms,
};

pub(super) const ANSWER_CORE_LIMIT: usize = 5;

/// How far retrieval may walk from something the question actually matched.
/// Two hops covers `symptom → decision → constraint`, the shape a root-cause
/// question needs, without opening the whole neighbourhood.
const MAX_REACH_HOPS: u32 = 2;
const MAX_REACHED_REFS: usize = 8;
const MAX_RESCUED_CANDIDATES: usize = 5;
const MAX_ASSOCIATED_CANDIDATES: usize = 3;

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
        let morphology = &self.context.morphology;
        let question_terms = informative_terms(question, morphology);
        if question_terms.is_empty() {
            let mut evidence = evidence;
            evidence.sort_by_key(stable_evidence_key);
            return evidence;
        }

        let strict_focus = match policy {
            MemoryAnswerPolicy::EvidenceOrUnknown | MemoryAnswerPolicy::ShowConflicts => {
                let terms = strict_answer_focus_terms(question, morphology);
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
        let lexicon = Lexicon::build(question, morphology, &prepared);

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

        let (associated, rejected) = self.associated_candidates(rejected, &lexicon);
        answer.extend(self.reached_candidates(&answer, rejected));
        answer.extend(associated);
        answer
    }

    /// Rescues what this memory's own vocabulary connects to the question.
    ///
    /// Same shape as the relation walk and for the same reason: the words a
    /// store keeps using together are evidence about that store, not a claim
    /// the reader made. These arrive marked and stay out of the answer core,
    /// so an association can carry retrieval and still cannot answer.
    fn associated_candidates(
        &self,
        rejected: Vec<MemoryEvidence>,
        lexicon: &Lexicon,
    ) -> (Vec<MemoryEvidence>, Vec<MemoryEvidence>) {
        let mut associated = Vec::new();
        let mut still_rejected = Vec::new();
        for item in rejected {
            let terms = AnswerCandidateTerms::from_evidence(&item, &self.context);
            let current =
                self.context.temporal_state(&item) == CandidateTemporalState::CurrentOrUnspecified;
            if current && lexicon.is_associated(&terms) {
                associated.push((lexicon.direct_score(&terms), item));
            } else {
                still_rejected.push(item);
            }
        }
        associated.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| stable_evidence_key(left).cmp(&stable_evidence_key(right)))
        });
        (
            associated
                .into_iter()
                .take(MAX_ASSOCIATED_CANDIDATES)
                .map(|(_, item)| mark_reached_by(item, REACHED_BY_ASSOCIATION))
                .collect(),
            still_rejected,
        )
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
        let question_terms = informative_terms(question, &self.context.morphology);
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

    /// The question's own words that reached the retained evidence.
    ///
    /// Reported as the caller wrote them, folded but not renamed. Matching
    /// happens on the search key — a concept the table unified, or a stem —
    /// and reporting that key instead would answer with the kernel's internal
    /// vocabulary rather than the reader's.
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
        informative_tokens(question)
            .filter(|token| evidence_terms.contains(&search_key(token, &self.context.morphology)))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub(super) fn matched_relations(
        &self,
        question: &str,
        evidence: &[MemoryEvidence],
    ) -> Vec<String> {
        let question_terms = informative_terms(question, &self.context.morphology);
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use kmp_domain::{
        BundleMetadata, BundleNode, BundleRelationship, CaseId, RelationExplanation,
        RelationSemanticClass, Role,
    };
    use prost_types::Timestamp;

    use super::super::answer_selection::{REACHED_BY_KEY, was_reached_indirectly};
    use super::super::morphology::Morphology;
    use super::super::relation_direction::RelationDirection;
    use super::super::relation_feature::RelationFeature;
    use super::super::search_terms::{fold_search_term, terms_match};

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
            endpoint_terms: informative_terms("source target", &Morphology::none()),
            why_terms: informative_terms(why, &Morphology::none()),
            evidence_terms: BTreeSet::new(),
            relation_terms: informative_terms(rel, &Morphology::none()),
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
    /// A bundle whose prose is the language the ranker will read.
    fn ranker_speaking(prose: &[&str]) -> AnswerEvidenceRanker {
        let node = |id: &str, summary: &str| {
            BundleNode::new(
                id,
                "memory",
                id,
                summary,
                "ACTIVE",
                Vec::new(),
                BTreeMap::new(),
            )
        };
        let bundle = KmpBundle::new(
            CaseId::new("about:memoria").expect("case id"),
            Role::new("answerer").expect("role"),
            node("about:memoria", prose[0]),
            prose
                .iter()
                .enumerate()
                .skip(1)
                .map(|(index, text)| node(&format!("entry:{index}"), text))
                .collect(),
            Vec::new(),
            Vec::new(),
            BundleMetadata::initial("test"),
        )
        .expect("bundle");
        AnswerEvidenceRanker::from_bundle(&bundle)
    }

    /// Plural against singular and one conjugation against another. Neither
    /// pair is in the hand-kept table, and the table has no Spanish at all.
    #[test]
    fn a_spanish_question_reaches_the_other_shapes_of_the_same_word() {
        let ranker = ranker_speaking(&[
            "La valvula de reserva se congelo durante la noche de la guardia.",
            "El despliegue de la pasarela se hizo por la manana con el equipo.",
        ]);

        let ranked = ranker.rank(
            "Que valvulas se congelaron?",
            MemoryAnswerPolicy::BestEffort,
            vec![
                claim_ev("valve", "claim:valve", "La valvula de reserva se congelo."),
                claim_ev("other", "claim:other", "El equipo reviso la pasarela."),
            ],
        );

        assert_eq!(ranked[0].id, "detail:valve");
    }

    #[test]
    fn an_english_question_reaches_the_noun_from_the_verb() {
        let ranker = ranker_speaking(&[
            "The deploy of the gateway was frozen during the audit of the week.",
            "The weekly meeting was moved to ten in the morning by the team.",
        ]);

        let ranked = ranker.rank(
            "What did the deployment freeze?",
            MemoryAnswerPolicy::BestEffort,
            vec![
                claim_ev("gateway", "claim:gateway", "The deploy was frozen."),
                claim_ev("other", "claim:other", "The team reviewed the gateway."),
            ],
        );

        assert_eq!(ranked[0].id, "detail:gateway");
    }

    /// Nothing is stemmed when no language can be read, which is what every
    /// caller got before morphology existed.
    #[test]
    fn a_memory_whose_language_cannot_be_read_keeps_exact_matching() {
        let ranker = ranker_speaking(&["Valkey.", "SQLite."]);

        let ranked = ranker.rank(
            "Que valvulas se congelaron?",
            MemoryAnswerPolicy::BestEffort,
            vec![claim_ev(
                "valve",
                "claim:valve",
                "La valvula de reserva se congelo.",
            )],
        );

        assert!(ranked.is_empty());
    }

    /// Nothing links the question's word to the answer's but this store's
    /// habit of using them in the same entry.
    #[test]
    fn the_stores_own_vocabulary_can_carry_a_question_to_a_memory_its_words_missed() {
        // The pair recurs across different sentences, which is what separates
        // two words that travel together from two that shared one sentence.
        let together = [
            "The cache now runs on valkey in staging.",
            "Valkey backs the cache for the checkout service.",
            "We moved the cache onto valkey last quarter.",
        ];
        let mut candidates = together
            .iter()
            .enumerate()
            .map(|(index, text)| {
                claim_ev(
                    &format!("pair-{index}"),
                    &format!("claim:pair{index}"),
                    text,
                )
            })
            .collect::<Vec<_>>();
        let apart = [
            "The weekly meeting moved to ten in the morning.",
            "The invoice from the supplier arrived late.",
            "The canteen menu changed on Tuesday.",
            "A new laptop was ordered for the intern.",
            "The fire drill is scheduled for Thursday.",
            "The parking permits were renewed.",
            "The printer on the second floor jammed.",
            "The office plants were watered.",
            "The badge reader was replaced at reception.",
        ];
        candidates.extend(apart.iter().enumerate().map(|(index, text)| {
            claim_ev(
                &format!("noise-{index}"),
                &format!("claim:noise{index}"),
                text,
            )
        }));
        candidates.push(claim_ev(
            "target",
            "claim:target",
            "Valkey was benchmarked overnight and kept.",
        ));

        let ranked = AnswerEvidenceRanker::default().rank(
            "What was chosen for the cache?",
            MemoryAnswerPolicy::BestEffort,
            candidates,
        );

        let target = ranked
            .iter()
            .find(|item| item.id == "detail:target")
            .expect("the association should carry the question to the target");
        assert!(was_reached_indirectly(target));
        assert_eq!(target.metadata[REACHED_BY_KEY], REACHED_BY_ASSOCIATION);
    }

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
        assert!(was_reached_indirectly(&ranked[1]));
        assert_eq!(ranked[1].metadata["reached_from"], "claim:outage");
        assert_eq!(ranked[1].metadata["reached_via"], "triggers");
        assert_eq!(ranked[1].metadata["reached_hops"], "1");
        assert!(!was_reached_indirectly(&ranked[0]));
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
                        why_terms: informative_terms("shared migration", &Morphology::none()),
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
        assert!(
            feature
                .evidence_terms
                .contains(ranker.context.morphology.stem("architecture").as_ref())
        );
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
        let terms = informative_terms("PR #83 C1 M1 P0 ID 7 is to un", &Morphology::none());
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

        assert_eq!(
            informative_terms("VÁLVULA", &Morphology::none()),
            informative_terms("valvula", &Morphology::none())
        );
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
        let question_terms = informative_terms("new installation", &Morphology::none());
        let evidence_terms = informative_terms("fresh build", &Morphology::none());

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
