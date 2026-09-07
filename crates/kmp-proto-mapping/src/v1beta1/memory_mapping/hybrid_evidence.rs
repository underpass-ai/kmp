use std::collections::{BTreeMap, BTreeSet};

use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_ranker::ANSWER_CORE_LIMIT;
use super::answer_selection::was_reached_indirectly;

/// Keep the deterministic answer core, then fuse proof rankings with RRF.
/// A duplicate gets one vote per channel. Semantic-only items keep their
/// indirect-retrieval mark and cannot enter `because` or raise confidence.
pub(super) fn fuse_evidence(
    lexical: Vec<MemoryEvidence>,
    supplemental: Vec<Vec<MemoryEvidence>>,
) -> Vec<MemoryEvidence> {
    if supplemental.iter().all(Vec::is_empty) {
        return lexical;
    }
    let core = lexical
        .iter()
        .filter(|item| !was_reached_indirectly(item))
        .take(ANSWER_CORE_LIMIT)
        .cloned()
        .collect::<Vec<_>>();
    let core_ids = core
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let mut items = BTreeMap::new();
    let mut scores = BTreeMap::<String, f64>::new();
    // Insert lexical objects first, retaining their existing provenance when
    // both channels return the same stored item.
    for ranking in std::iter::once(lexical).chain(supplemental) {
        let mut seen = BTreeSet::new();
        for item in ranking {
            if !seen.insert(item.id.clone()) {
                continue;
            }
            let score = 1.0 / (60.0 + seen.len() as f64);
            if !core_ids.contains(&item.id) {
                *scores.entry(item.id.clone()).or_default() += score;
                items.entry(item.id.clone()).or_insert(item);
            }
        }
    }
    let mut ranked = items.into_values().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        scores[&right.id]
            .total_cmp(&scores[&left.id])
            .then_with(|| left.id.cmp(&right.id))
    });
    core.into_iter().chain(ranked).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, indirect: bool) -> MemoryEvidence {
        let mut item = MemoryEvidence {
            id: id.into(),
            text: id.into(),
            ..Default::default()
        };
        if indirect {
            item.metadata.insert("reached_by".into(), "semantic".into());
        }
        item
    }

    #[test]
    fn similarity_rescues_evidence_without_displacing_citations_or_duplicate_votes() {
        let lexical = (0..7)
            .map(|n| item(&n.to_string(), false))
            .collect::<Vec<_>>();
        let semantic = vec![item("new", true), item("6", true), item("6", true)];
        let combined = fuse_evidence(lexical, vec![semantic]);
        assert_eq!(
            combined
                .iter()
                .take(5)
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["0", "1", "2", "3", "4"]
        );
        assert_eq!(combined.len(), 8);
        assert_eq!(combined[5].id, "6");
        assert!(!was_reached_indirectly(&combined[5]));
        assert!(was_reached_indirectly(
            combined
                .iter()
                .find(|i| i.id == "new")
                .expect("valid test fixture")
        ));
    }

    #[test]
    fn independent_channels_vote_once_without_fusing_their_ranks_first() {
        let graph = vec![item("association", true)];
        let dense = vec![item("dense-only", true), item("agreement", true)];
        let bm25 = vec![item("agreement", true), item("lexical-only", true)];
        let ranked = fuse_evidence(graph.clone(), vec![dense.clone(), bm25.clone()]);
        assert_eq!(ranked[0].id, "agreement");
        assert!(ranked.iter().all(was_reached_indirectly));
        let mut duplicated = bm25;
        duplicated.insert(0, item("agreement", true));
        let repeated = fuse_evidence(graph, vec![dense, duplicated]);
        assert_eq!(
            ranked, repeated,
            "duplicate transport rows cannot add channel votes"
        );
        assert_eq!(ranked.len(), 4);
    }

    #[test]
    fn additional_lexical_proof_does_not_displace_the_answer_core() {
        let core = (0..5)
            .map(|n| item(&format!("core{n}"), false))
            .collect::<Vec<_>>();
        let ranked = fuse_evidence(
            core.clone(),
            vec![vec![item("other", true)], vec![item("other", true)]],
        );
        assert_eq!(&ranked[..5], core.as_slice());
        assert_eq!(ranked[5].id, "other");
        assert!(was_reached_indirectly(&ranked[5]));
    }
}
