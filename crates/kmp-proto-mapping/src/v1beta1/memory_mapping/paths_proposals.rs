use std::collections::{BTreeMap, BTreeSet, VecDeque};

use kmp_domain::DeclaredEdge;

use super::partner_shortlist::PartnerShortlist;
use super::relate_proposals::FactWords;

/// Declared hops a paths reading's candidates may lie from either end.
const RADIUS: usize = 4;
/// Facts sharing the ends' rarer words a paths reading also compares.
const LEXICAL: usize = 64;

/// Which facts the kernel's pair proposals compare for a `kmp_curate` path
/// search (DESIGN L7). Every pair of a large about is quadratic: 19 s and
/// 3.7 GB at 10^4 facts, all of it in the proposer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathsProposals {
    /// Every pair, as a review reads them: a goal-less search with a judge.
    All,
    /// No pair: a search without a judge walks declared relations alone and
    /// never reads a proposal.
    Nothing,
    /// Only the facts a goal search's corridor can hold: those within four
    /// declared hops of either end, the 64 that share most of the two ends'
    /// rarer words, and the ends.
    Around { from: String, to: String },
}

impl PathsProposals {
    /// The facts of `words` the proposer compares; `None` compares them all.
    pub(super) fn candidates(
        &self,
        words: &[FactWords],
        declared: &[DeclaredEdge],
    ) -> Option<BTreeSet<String>> {
        let (from, to) = match self {
            Self::All => return None,
            Self::Nothing => return Some(BTreeSet::new()),
            Self::Around { from, to } => (from.as_str(), to.as_str()),
        };
        let mut adjacency = BTreeMap::<&str, Vec<&str>>::new();
        for edge in declared {
            adjacency.entry(&edge.from).or_default().push(&edge.to);
            adjacency.entry(&edge.to).or_default().push(&edge.from);
        }
        let mut kept = BTreeSet::new();
        for end in [from, to] {
            let mut distance = BTreeMap::from([(end, 0usize)]);
            let mut queue = VecDeque::from([end]);
            while let Some(node) = queue.pop_front() {
                kept.insert(node.to_string());
                if distance[node] == RADIUS {
                    continue;
                }
                for next in adjacency.get(node).into_iter().flatten() {
                    if !distance.contains_key(next) {
                        distance.insert(next, distance[node] + 1);
                        queue.push_back(next);
                    }
                }
            }
        }
        let text = |reference: &str| {
            words
                .iter()
                .find(|fact| fact.ref_id == reference)
                .map(|fact| fact.text.as_str())
                .unwrap_or_default()
        };
        let ends = format!("{} {}", text(from), text(to));
        let shortlist = PartnerShortlist::over(
            words
                .iter()
                .map(|fact| (fact.ref_id.as_str(), fact.text.as_str())),
        );
        kept.extend(
            shortlist
                .partners(from, &ends, LEXICAL)
                .refs()
                .take(LEXICAL)
                .cloned(),
        );
        Some(kept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(texts: &[(&str, &str)]) -> Vec<FactWords> {
        texts
            .iter()
            .map(|(reference, text)| FactWords {
                ref_id: (*reference).into(),
                about: "a".into(),
                text: (*text).into(),
                summary_en: None,
            })
            .collect()
    }

    fn declared(from: &str, to: &str) -> DeclaredEdge {
        DeclaredEdge {
            from: from.into(),
            to: to.into(),
            rel: "triggers".into(),
            why: "w".into(),
            evidence: "e".into(),
        }
    }

    #[test]
    fn around_keeps_the_declared_balls_the_ends_words_and_the_ends() {
        let mut texts = vec![
            ("s", "The Valkey failover broke billing."),
            ("g", "Refunds after the Valkey billing outage."),
            ("a", "Retries piled up."),
            ("w", "Valkey billing postmortem."),
        ];
        let lunch = (0..100)
            .map(|n| (format!("x{n:03}"), format!("Team lunch {n}.")))
            .collect::<Vec<_>>();
        texts.extend(lunch.iter().map(|(r, t)| (r.as_str(), t.as_str())));
        let words = words(&texts);
        let edges = [declared("s", "a"), declared("x050", "x051")];
        let around = PathsProposals::Around {
            from: "s".into(),
            to: "g".into(),
        }
        .candidates(&words, &edges)
        .expect("a subset");
        for kept in ["s", "g", "a", "w"] {
            assert!(around.contains(kept), "{kept}: {around:?}");
        }
        assert!(around.len() <= 2 + 2 + LEXICAL, "{}", around.len());
        assert_eq!(PathsProposals::All.candidates(&words, &edges), None);
        assert_eq!(
            PathsProposals::Nothing.candidates(&words, &edges),
            Some(BTreeSet::new())
        );
    }
}
