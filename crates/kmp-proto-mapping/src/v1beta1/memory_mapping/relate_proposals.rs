//! Proposed links between facts of different abouts: the shared key.
//!
//! Two abouts that both mention `#469`, that summarise the same thing in
//! English, or that both name `Valkey` inside one span are candidates for
//! being about the same thing. The kernel proposes it with proof and stores
//! nothing; a writer decides. Every signal is measured against the span's
//! own collection, so a token common to everything in it — a year, a
//! product name every entry carries — joins nothing.

use std::collections::{BTreeMap, BTreeSet};

use kmp_domain::language::{identifiers, proper_names};
use kmp_domain::{ProposalSignal, ProposedLink, RelatedFact, cap_proposals_per_fact};

use super::lexical_bridge::LexicalBridge;
use super::morphology::Morphology;
use super::search_terms::informative_terms;

/// The share of concepts two English summaries must have in common.
const SUMMARY_SHARE_FLOOR: f64 = 0.6;
/// And how many they must share outright, so two one-word summaries never
/// match on the one word.
const SUMMARY_SHARED_MINIMUM: usize = 2;
/// The table cosine at which two concepts count as one across a language.
const BRIDGE_SIMILARITY_FLOOR: f64 = 0.45;

/// What a fact says, as the proposals read it: its text, and its English
/// summary when the writer gave one.
pub(super) struct FactWords {
    pub(super) ref_id: String,
    pub(super) about: String,
    pub(super) text: String,
    pub(super) summary_en: Option<String>,
}

pub(super) fn propose_links(
    facts: &[RelatedFact],
    words: &[FactWords],
    morphology: &Morphology,
    bridge: &LexicalBridge,
) -> Vec<ProposedLink> {
    let scopes_by_ref = facts
        .iter()
        .map(|fact| (fact.ref_id().to_string(), fact.bare_scopes()))
        .collect::<BTreeMap<_, _>>();
    let identifiers_by_ref = words
        .iter()
        .map(|fact| (fact.ref_id.as_str(), identifiers(&fact.text)))
        .collect::<BTreeMap<_, _>>();
    let rare = rare_identifiers(&identifiers_by_ref, words.len());
    let concepts_by_ref = words
        .iter()
        .filter_map(|fact| {
            let summary = fact.summary_en.as_deref()?;
            let concepts = informative_terms(summary, morphology);
            (!concepts.is_empty()).then_some((fact.ref_id.as_str(), concepts))
        })
        .collect::<BTreeMap<_, _>>();
    let names_by_ref = words
        .iter()
        .map(|fact| (fact.ref_id.as_str(), proper_names(&fact.text)))
        .collect::<BTreeMap<_, _>>();

    let mut proposals = Vec::new();
    for (index, first) in words.iter().enumerate() {
        for second in &words[index + 1..] {
            if first.about == second.about {
                continue;
            }
            let mut signals = Vec::new();
            let shared = identifiers_by_ref[first.ref_id.as_str()]
                .intersection(&identifiers_by_ref[second.ref_id.as_str()])
                .filter_map(|identifier| rare.get(identifier).map(|idf| (identifier.clone(), *idf)))
                .collect::<Vec<_>>();
            if !shared.is_empty() {
                let idf = shared.iter().map(|(_, idf)| *idf).fold(0.0, f64::max);
                signals.push(ProposalSignal::Identifier {
                    shared: shared
                        .into_iter()
                        .map(|(identifier, _)| identifier)
                        .collect(),
                    idf,
                });
            }
            if let (Some(left), Some(right)) = (
                concepts_by_ref.get(first.ref_id.as_str()),
                concepts_by_ref.get(second.ref_id.as_str()),
            ) && let Some(signal) = summary_signal(left, right, bridge)
            {
                signals.push(signal);
            }
            let entities = names_by_ref[first.ref_id.as_str()]
                .intersection(&names_by_ref[second.ref_id.as_str()])
                .cloned()
                .collect::<Vec<_>>();
            if !entities.is_empty() {
                signals.push(ProposalSignal::Entity { entities });
            }
            if signals.is_empty() {
                continue;
            }
            let scope = scopes_by_ref.get(&first.ref_id).and_then(|left| {
                scopes_by_ref
                    .get(&second.ref_id)
                    .and_then(|right| left.intersection(right).next().cloned())
            });
            proposals.push(ProposedLink::new(
                first.ref_id.clone(),
                second.ref_id.clone(),
                signals,
                scope,
            ));
        }
    }
    cap_proposals_per_fact(proposals)
}

/// The identifiers rare enough across the span to say something: carried by
/// at least two facts, by fewer than all of them, and at or above the
/// median rarity of every identifier two facts share. `#469` in two of ten
/// facts informs; `2026` in all ten does not.
fn rare_identifiers(
    identifiers_by_ref: &BTreeMap<&str, BTreeSet<String>>,
    facts: usize,
) -> BTreeMap<String, f64> {
    let mut frequency = BTreeMap::<&str, usize>::new();
    for identifiers in identifiers_by_ref.values() {
        for identifier in identifiers {
            *frequency.entry(identifier.as_str()).or_default() += 1;
        }
    }
    let idf = |count: usize| (facts as f64 / count as f64).ln();
    let mut rarities = frequency
        .iter()
        .filter(|(_, count)| **count >= 2)
        .map(|(_, count)| idf(*count))
        .collect::<Vec<_>>();
    if rarities.is_empty() {
        return BTreeMap::new();
    }
    rarities.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let middle = rarities.len() / 2;
    let median = if rarities.len() % 2 == 0 {
        (rarities[middle - 1] + rarities[middle]) / 2.0
    } else {
        rarities[middle]
    };
    frequency
        .into_iter()
        .filter(|(_, count)| *count >= 2 && *count < facts)
        .map(|(identifier, count)| (identifier.to_string(), idf(count)))
        .filter(|(_, rarity)| *rarity >= median && *rarity > 0.0)
        .collect()
}

/// Two summaries match when they share most of their concepts, outright or
/// through the table, and share at least two outright or bridged.
fn summary_signal(
    left: &BTreeSet<String>,
    right: &BTreeSet<String>,
    bridge: &LexicalBridge,
) -> Option<ProposalSignal> {
    let shared_terms = left.intersection(right).cloned().collect::<Vec<_>>();
    let mut bridged = Vec::new();
    if !bridge.is_silent() {
        let mut taken = BTreeSet::new();
        for a in left.difference(right) {
            for b in right.difference(left) {
                if taken.contains(b) {
                    continue;
                }
                if bridge
                    .similarity(a, b)
                    .is_some_and(|cosine| cosine >= BRIDGE_SIMILARITY_FLOOR)
                {
                    bridged.push(format!("{a}≈{b}"));
                    taken.insert(b.clone());
                    break;
                }
            }
        }
    }
    let total = shared_terms.len() + bridged.len();
    let smaller = left.len().min(right.len());
    if smaller == 0 || total < SUMMARY_SHARED_MINIMUM {
        return None;
    }
    let share = total as f64 / smaller as f64;
    (share >= SUMMARY_SHARE_FLOOR).then_some(ProposalSignal::Summary {
        shared_terms,
        bridged,
        share,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identifiers_of(texts: &[(&str, &str)]) -> BTreeMap<&'static str, BTreeSet<String>> {
        texts
            .iter()
            .map(|(ref_id, text)| {
                let ref_id: &'static str = Box::leak(ref_id.to_string().into_boxed_str());
                (ref_id, identifiers(text))
            })
            .collect()
    }

    #[test]
    fn a_ticket_two_facts_share_is_rare_and_a_year_every_fact_carries_is_not() {
        let by_ref = identifiers_of(&[
            ("a:1", "Ticket #469 blocks the 2026 release."),
            ("b:1", "The fix for #469 shipped in 2026."),
            ("a:2", "The 2026 canteen menu was posted."),
            ("b:2", "Planning for 2026 closed."),
        ]);
        let rare = rare_identifiers(&by_ref, 4);
        assert!(rare.contains_key("#469"), "{rare:?}");
        assert!(
            !rare.contains_key("2026"),
            "carried by every fact: {rare:?}"
        );
    }
}
