use kmp_proto::v1beta1::MemoryEvidence;

use super::content_scores::ContentScores;
use super::gate_verdict::GateVerdict;
use super::ranked_evidence::RankedEvidence;

/// What the ranker read for a question that names an identifier: the proof,
/// and the anchored gate's verdict when a required anchor decided it.
pub(super) struct AnchoredReading {
    /// The cited core first, then the rest in rank order.
    pub(super) evidence: Vec<MemoryEvidence>,
    /// Where the head ends (P14, [`RankedEvidence`]): the cited core and
    /// what was in the ranking's head.
    pub(super) head: usize,
    /// `None` when no required anchor survived: the ordinary rule decides.
    pub(super) verdict: Option<GateVerdict>,
    /// The content score of each candidate the ranking reached directly.
    pub(super) scores: ContentScores,
    /// The ids the anchored gate may cite (direct candidates naming the
    /// principal anchor), in rank order; empty when no anchor decided.
    pub(super) promotable: Vec<String>,
}

impl AnchoredReading {
    pub(super) fn unanchored(ranked: RankedEvidence, scores: ContentScores) -> Self {
        Self {
            evidence: ranked.evidence,
            head: ranked.head,
            verdict: None,
            scores,
            promotable: Vec::new(),
        }
    }

    /// The verdict's citations move to the front, in the order the verdict
    /// cites them (the order they were ranked, then any a judge promoted),
    /// so a cap on the proof never cuts a citation it keeps.
    pub(super) fn decided(ranked: RankedEvidence, verdict: GateVerdict) -> Self {
        let cited = verdict.cited();
        let mut core = Vec::new();
        let mut rest = Vec::new();
        let mut rest_in_head = 0;
        for (index, item) in ranked.evidence.into_iter().enumerate() {
            if cited.contains(item.id.as_str()) {
                core.push(item);
            } else {
                rest_in_head += usize::from(index < ranked.head);
                rest.push(item);
            }
        }
        core.sort_by_key(|item| verdict.core.iter().position(|id| *id == item.id));
        let head = core.len() + rest_in_head;
        core.extend(rest);
        Self {
            head,
            evidence: core,
            verdict: Some(verdict),
            scores: ContentScores::default(),
            promotable: Vec::new(),
        }
    }

    pub(super) fn with_scores(mut self, scores: ContentScores) -> Self {
        self.scores = scores;
        self
    }

    pub(super) fn with_promotable(mut self, promotable: Vec<String>) -> Self {
        self.promotable = promotable;
        self
    }
}
