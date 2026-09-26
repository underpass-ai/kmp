use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::{LifecycleChain, LifecycleStep};
use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_candidate::reached_by_the_question;
use super::answer_candidate_terms::AnswerCandidateTerms;
use super::answer_recall_context::AnswerRecallContext;
use super::answer_selection::{
    LIFECYCLE_HEADS_KEY, LIFECYCLE_STATE_KEY, REACHED_BY_LIFECYCLE, answer_context_refs,
    mark_reached_by, stable_evidence_key,
};
use super::candidate_temporal_state::CandidateTemporalState;
use super::lexicon::Lexicon;
use super::lifecycle_ask::LifecycleAsk;
use super::ranking_focus::RankingFocus;

/// How many current heads a question about now may bring in.
pub(super) const MAX_SUCCESSORS_RESCUED: usize = 3;
/// How many chain members a question about history may bring in.
pub(super) const MAX_CHAIN_RESCUED: usize = 8;
/// How many matched memories a rescue walks from before it stops looking.
const MAX_LIFECYCLE_SEEDS: usize = 32;

/// A candidate with the terms the ranker read it with.
type ReadCandidate = (MemoryEvidence, AnswerCandidateTerms);

/// Brings in what a declared lifecycle connects to what the question
/// matched.
///
/// `eligible()` withholds a replaced memory from a question about now, which
/// is right, and used to leave the reader with nothing when the replacement
/// did not repeat the question's words. The replacement is found by
/// following the writer's own `supersedes`, `corrects` and `updates_state`
/// from the memory the question matched to the head of its chain
/// ([`LifecycleChain`]). It arrives marked `reached_by: lifecycle`, after
/// every memory the question matched in its own words and outside the
/// answer core, like every other rescue: a lifecycle proves succession, not
/// that the successor answers. A replaced memory never comes back as
/// current: a question about now brings only heads that still stand, and a
/// question about history brings the chain with each member's state said.
///
/// Nothing happens in a bundle that declares no lifecycle, so its answers
/// stay byte for byte what they were.
pub(super) struct LifecycleRescue<'a> {
    pub(super) context: &'a AnswerRecallContext,
    pub(super) question: &'a str,
    pub(super) focus: RankingFocus<'a>,
    pub(super) lexicon: &'a Lexicon,
}

/// A memory the question matched, from which a lifecycle is walked.
struct Seed {
    node: String,
    /// Whether the question reached it in its own words while the ranker
    /// withheld it for its lifecycle.
    withheld: bool,
}

/// A memory a lifecycle walk found for the answer.
struct Found {
    node: String,
    seed: String,
    via: Option<String>,
    hops: usize,
    heads: Vec<String>,
}

impl LifecycleRescue<'_> {
    /// The rescued memories in the order they were found, and what stays
    /// rejected.
    pub(super) fn rescue(
        &self,
        answer: &[MemoryEvidence],
        rejected: Vec<ReadCandidate>,
    ) -> (Vec<MemoryEvidence>, Vec<ReadCandidate>) {
        let links = &self.context.lifecycle_links;
        if links.is_empty() || rejected.is_empty() {
            return (Vec::new(), rejected);
        }
        let ask = LifecycleAsk::read(
            self.question,
            &self.context.morphology,
            self.context.lifecycle.reads_an_instant(),
        );
        let answered = answer
            .iter()
            .flat_map(answer_context_refs)
            .collect::<BTreeSet<_>>();
        let seeds = self.seeds(ask, answer, &rejected);
        if seeds.is_empty() {
            return (Vec::new(), rejected);
        }
        let pool = pool_by_ref(&rejected);
        let limit = match ask {
            LifecycleAsk::Current => MAX_SUCCESSORS_RESCUED,
            LifecycleAsk::History => MAX_CHAIN_RESCUED,
        };
        let mut found = Vec::<Found>::new();
        let mut taken = BTreeSet::<usize>::new();
        for seed in seeds {
            if found.len() == limit {
                break;
            }
            let walked = match ask {
                LifecycleAsk::Current => LifecycleChain::successors(&seed.node, links),
                LifecycleAsk::History => LifecycleChain::walk(&seed.node, links),
            };
            // An in-memory source cannot fail; a walk that did is no rescue.
            let Ok(chain) = walked else {
                continue;
            };
            let members = match ask {
                LifecycleAsk::Current => self.standing_heads(&chain, &pool, &answered),
                LifecycleAsk::History => chain_members(&chain, seed.withheld),
            };
            let heads = if chain.heads().len() > 1 {
                chain.heads().to_vec()
            } else {
                Vec::new()
            };
            for (node, step) in members {
                if found.len() == limit {
                    break;
                }
                if answered.contains(&node) {
                    continue;
                }
                let Some(&index) = pool.get(&node) else {
                    continue;
                };
                if !taken.insert(index) {
                    continue;
                }
                found.push(Found {
                    node,
                    seed: seed.node.clone(),
                    via: step.map(|step| step.via.as_str().to_string()),
                    hops: step.map_or(0, |step| step.depth),
                    heads: heads.clone(),
                });
            }
        }
        if found.is_empty() {
            return (Vec::new(), rejected);
        }
        let mut slots = rejected.into_iter().map(Some).collect::<Vec<_>>();
        let rescued = found
            .into_iter()
            .filter_map(|found| {
                let index = pool[&found.node];
                let (item, _) = slots[index].take()?;
                Some(self.mark(item, &found, ask))
            })
            .collect();
        (rescued, slots.into_iter().flatten().collect())
    }

    /// What the walks start from, in the order they are walked: what the
    /// answer cites, in its order, then what the question matched and the
    /// ranker withheld for its lifecycle, strongest match first.
    fn seeds(
        &self,
        ask: LifecycleAsk,
        answer: &[MemoryEvidence],
        rejected: &[ReadCandidate],
    ) -> Vec<Seed> {
        let links = &self.context.lifecycle_links;
        let mut seen = BTreeSet::new();
        let mut seeds = Vec::new();
        let walkable = |node: &str| match ask {
            LifecycleAsk::Current => links.has_newer(node),
            LifecycleAsk::History => links.touches(node),
        };
        for item in answer {
            for node in answer_context_refs(item) {
                if walkable(&node) && seen.insert(node.clone()) {
                    seeds.push(Seed {
                        node,
                        withheld: false,
                    });
                }
            }
        }
        let mut withheld = rejected
            .iter()
            .filter(|(item, _)| {
                self.context.temporal_state(item) != CandidateTemporalState::CurrentOrUnspecified
            })
            .filter(|(_, terms)| reached_by_the_question(self.focus, self.lexicon, terms))
            .map(|(item, terms)| (self.lexicon.direct_score(terms), item))
            .collect::<Vec<_>>();
        withheld.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| stable_evidence_key(left).cmp(&stable_evidence_key(right)))
        });
        for (_, item) in withheld {
            for node in answer_context_refs(item) {
                if walkable(&node) && seen.insert(node.clone()) {
                    seeds.push(Seed {
                        node,
                        withheld: true,
                    });
                }
            }
        }
        seeds.truncate(MAX_LIFECYCLE_SEEDS);
        seeds
    }

    /// The newest members of a chain that still stand and that the answer
    /// could cite: the heads, or, when a head lies outside what the
    /// selection admitted, the newest standing member before it.
    fn standing_heads<'c>(
        &self,
        chain: &'c LifecycleChain,
        pool: &BTreeMap<String, usize>,
        answered: &BTreeSet<String>,
    ) -> Vec<(String, Option<&'c LifecycleStep>)> {
        let standing = chain
            .newer()
            .iter()
            .filter(|step| pool.contains_key(&step.node) || answered.contains(&step.node))
            .filter(|step| self.stands(&step.node))
            .collect::<Vec<_>>();
        let behind_another = standing
            .iter()
            .flat_map(|step| ancestors(chain, step))
            .collect::<BTreeSet<_>>();
        standing
            .into_iter()
            .filter(|step| !behind_another.contains(step.node.as_str()))
            .map(|step| (step.node.clone(), Some(step)))
            .collect()
    }

    fn stands(&self, node: &str) -> bool {
        !self.context.lifecycle.is_superseded(node) && !self.context.lifecycle.is_expired(node)
    }

    /// Marks a rescued memory with its route. Every page may repeat the
    /// largest item's bytes, so the marks say only what the route does not
    /// already imply: a question about now brings only standing heads, so
    /// their state goes unsaid, and one hop goes unsaid.
    fn mark(&self, item: MemoryEvidence, found: &Found, ask: LifecycleAsk) -> MemoryEvidence {
        let replaced = !answer_context_refs(&item)
            .iter()
            .all(|node| self.stands(node));
        let mut item = mark_reached_by(item, REACHED_BY_LIFECYCLE);
        let metadata = &mut item.metadata;
        metadata.insert("reached_from".to_string(), found.seed.clone());
        if let Some(via) = &found.via {
            metadata.insert("reached_via".to_string(), via.clone());
        }
        if ask == LifecycleAsk::History || found.hops > 1 {
            metadata.insert("reached_hops".to_string(), found.hops.to_string());
        }
        if ask == LifecycleAsk::History {
            let state = if replaced { "replaced" } else { "current" };
            metadata.insert(LIFECYCLE_STATE_KEY.to_string(), state.to_string());
        }
        if !found.heads.is_empty() {
            metadata.insert(LIFECYCLE_HEADS_KEY.to_string(), found.heads.join(","));
        }
        item
    }
}

/// Every member of a chain, oldest first, each with the step that reached
/// it; the seed itself only when the ranker withheld it, since otherwise the
/// answer already holds it.
fn chain_members(chain: &LifecycleChain, withheld: bool) -> Vec<(String, Option<&LifecycleStep>)> {
    let older = chain
        .older()
        .iter()
        .rev()
        .map(|step| (step.node.clone(), Some(step)));
    let seed = withheld.then(|| (chain.from().to_string(), None));
    let newer = chain
        .newer()
        .iter()
        .map(|step| (step.node.clone(), Some(step)));
    older.chain(seed).chain(newer).collect()
}

/// The memories between the walk's start and `step`, exclusive of both.
fn ancestors<'c>(chain: &'c LifecycleChain, step: &LifecycleStep) -> Vec<&'c str> {
    let by_node = chain
        .newer()
        .iter()
        .map(|step| (step.node.as_str(), step))
        .collect::<BTreeMap<_, _>>();
    let mut out = Vec::new();
    let mut current = by_node.get(step.from.as_str());
    while let Some(up) = current {
        out.push(up.node.as_str());
        current = by_node.get(up.from.as_str());
    }
    out
}

/// Each memory's own citation among the rejected candidates: its entry
/// text when it has one, else the first of its evidence by stable key.
fn pool_by_ref(rejected: &[ReadCandidate]) -> BTreeMap<String, usize> {
    let mut pool = BTreeMap::<String, usize>::new();
    for (index, (item, _)) in rejected.iter().enumerate() {
        for node in answer_context_refs(item) {
            let own_text = item.id == format!("entry:{node}");
            match pool.get(&node) {
                Some(&known) => {
                    let known_item = &rejected[known].0;
                    let known_own = known_item.id == format!("entry:{node}");
                    let better = (own_text && !known_own)
                        || (own_text == known_own
                            && stable_evidence_key(item) < stable_evidence_key(known_item));
                    if better {
                        pool.insert(node, index);
                    }
                }
                None => {
                    pool.insert(node, index);
                }
            }
        }
    }
    pool
}
