use kmp_application::MemoryAnswerPolicy;
use kmp_domain::TemporalSelection;
use kmp_proto::v1beta1::MemoryEvidence;

use super::gate_verdict::GateVerdict;
use super::lexical_bridge::LexicalBridge;
use super::ranked_selection::RankedSelection;

/// The anchored gate's reading of one question over one admitted pool: the
/// proof in the order the answer cites it, and the verdict.
///
/// A doubt band reads the question before any judge is asked. When it does
/// not enter the band, or the judge cannot answer, the answer is that very
/// reading, taken back for exactly the inputs it was read with.
pub(super) struct DecidedSelection {
    ranked: RankedSelection,
    verdict: Option<GateVerdict>,
}

impl DecidedSelection {
    pub(super) fn new(
        question: &str,
        policy: MemoryAnswerPolicy,
        temporal: &TemporalSelection,
        bridge: &LexicalBridge,
        evidence: Vec<MemoryEvidence>,
        verdict: Option<GateVerdict>,
    ) -> Self {
        Self {
            ranked: RankedSelection::new(question, policy, temporal, bridge, evidence),
            verdict,
        }
    }

    /// The reading, when it was taken for exactly these inputs.
    pub(super) fn into_decision_for(
        self,
        question: &str,
        policy: MemoryAnswerPolicy,
        temporal: &TemporalSelection,
        bridge: &LexicalBridge,
    ) -> Option<(Vec<MemoryEvidence>, Option<GateVerdict>)> {
        let verdict = self.verdict;
        self.ranked
            .into_ranking_for(question, policy, temporal, bridge)
            .map(|evidence| (evidence, verdict))
    }
}

#[cfg(test)]
mod tests {
    use kmp_proto::v1beta1::UnknownReason;

    use super::*;

    #[test]
    fn a_reading_is_taken_back_only_for_its_own_inputs() {
        let bridge = LexicalBridge::none();
        let temporal = TemporalSelection::default();
        let policy = MemoryAnswerPolicy::EvidenceOrUnknown;
        let verdict = GateVerdict::unknown(UnknownReason::NoBearing, Vec::new());
        let decided = || {
            DecidedSelection::new(
                "which port for #12",
                policy,
                &temporal,
                &bridge,
                vec![MemoryEvidence::default()],
                Some(verdict.clone()),
            )
        };
        assert_eq!(
            decided().into_decision_for("which port for #12", policy, &temporal, &bridge),
            Some((vec![MemoryEvidence::default()], Some(verdict.clone())))
        );
        assert!(
            decided()
                .into_decision_for("which port for #13", policy, &temporal, &bridge)
                .is_none()
        );
    }
}
