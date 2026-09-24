use super::candidate_pair::CandidatePair;
use super::declared_link::DeclaredLink;
use super::jev_verdict::JevVerdict;

/// One thing a review puts in front of the agent.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CurateFinding {
    /// A pair nothing declares. `suggested_rel` is Jev's choice, or none
    /// when Jev did not read it.
    Missing {
        pair: CandidatePair,
        suggested_rel: Option<String>,
        verdict: Option<JevVerdict>,
    },
    /// A declared relation whose stated reason Jev doubts, or whose type Jev
    /// would choose differently. Reported only.
    Suspect {
        link: DeclaredLink,
        support: f64,
        best: JevVerdict,
    },
}
