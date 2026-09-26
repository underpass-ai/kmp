use kmp_proto::v1beta1::MemoryEvidence;

use super::gate_verdict::GateVerdict;

/// What the ranker read for a question that names an identifier: the proof,
/// and the anchored gate's verdict when a required anchor decided it.
pub(super) struct AnchoredReading {
    /// The cited core first, then the rest in rank order.
    pub(super) evidence: Vec<MemoryEvidence>,
    /// `None` when no required anchor survived: the ordinary rule decides.
    pub(super) verdict: Option<GateVerdict>,
}

impl AnchoredReading {
    pub(super) fn unanchored(evidence: Vec<MemoryEvidence>) -> Self {
        Self {
            evidence,
            verdict: None,
        }
    }

    /// The verdict's citations move to the front, in the order they were
    /// ranked, so a cap on the proof never cuts a citation it keeps.
    pub(super) fn decided(evidence: Vec<MemoryEvidence>, verdict: GateVerdict) -> Self {
        let cited = verdict.cited();
        let (mut core, rest): (Vec<_>, Vec<_>) = evidence
            .into_iter()
            .partition(|item| cited.contains(item.id.as_str()));
        core.extend(rest);
        Self {
            evidence: core,
            verdict: Some(verdict),
        }
    }
}
