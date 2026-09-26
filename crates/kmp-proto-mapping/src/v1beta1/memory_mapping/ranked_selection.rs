use kmp_application::MemoryAnswerPolicy;
use kmp_domain::TemporalSelection;
use kmp_proto::v1beta1::MemoryEvidence;

use super::lexical_bridge::LexicalBridge;

/// The lexical ranker's order for one question over one admitted pool.
///
/// Ranking is a pure function of the question, the policy, the clock that
/// admits the pool, the table it bridges with and the bundle the pool comes
/// from. An Ask that builds a remote judge's pool ranks once; the answer
/// reads the same ranking back instead of ranking again, but only for the
/// very inputs it was ranked with. Anything else ranks afresh.
pub(super) struct RankedSelection {
    question: String,
    policy: MemoryAnswerPolicy,
    temporal: TemporalSelection,
    /// Identity of the table, compared and never dereferenced: one process
    /// holds one table for its life, so the address names it.
    bridge: usize,
    ranked: Vec<MemoryEvidence>,
}

impl RankedSelection {
    pub(super) fn new(
        question: &str,
        policy: MemoryAnswerPolicy,
        temporal: &TemporalSelection,
        bridge: &LexicalBridge,
        ranked: Vec<MemoryEvidence>,
    ) -> Self {
        Self {
            question: question.to_string(),
            policy,
            temporal: temporal.clone(),
            bridge: bridge_identity(bridge),
            ranked,
        }
    }

    pub(super) fn ranked(&self) -> &[MemoryEvidence] {
        &self.ranked
    }

    /// The ranking, when it was taken for exactly these inputs.
    pub(super) fn into_ranking_for(
        self,
        question: &str,
        policy: MemoryAnswerPolicy,
        temporal: &TemporalSelection,
        bridge: &LexicalBridge,
    ) -> Option<Vec<MemoryEvidence>> {
        (self.question == question
            && self.policy == policy
            && &self.temporal == temporal
            && self.bridge == bridge_identity(bridge))
        .then_some(self.ranked)
    }
}

fn bridge_identity(bridge: &LexicalBridge) -> usize {
    std::ptr::from_ref(bridge) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(id: &str) -> MemoryEvidence {
        MemoryEvidence {
            id: id.to_string(),
            text: format!("text of {id}"),
            ..Default::default()
        }
    }

    #[test]
    fn a_ranking_is_reused_only_for_the_inputs_it_was_taken_with() {
        let bridge = LexicalBridge::none();
        let other_bridge = LexicalBridge::none();
        let temporal = TemporalSelection::default();
        let policy = MemoryAnswerPolicy::EvidenceOrUnknown;
        let selection = || {
            RankedSelection::new(
                "why did it freeze",
                policy,
                &temporal,
                &bridge,
                vec![evidence("entry:a"), evidence("entry:b")],
            )
        };
        assert_eq!(selection().ranked().len(), 2);
        assert_eq!(
            selection().into_ranking_for("why did it freeze", policy, &temporal, &bridge),
            Some(vec![evidence("entry:a"), evidence("entry:b")])
        );
        assert!(
            selection()
                .into_ranking_for("why did it thaw", policy, &temporal, &bridge)
                .is_none()
        );
        assert!(
            selection()
                .into_ranking_for(
                    "why did it freeze",
                    MemoryAnswerPolicy::BestEffort,
                    &temporal,
                    &bridge
                )
                .is_none()
        );
        assert!(
            selection()
                .into_ranking_for("why did it freeze", policy, &temporal, &other_bridge)
                .is_none()
        );
    }
}
