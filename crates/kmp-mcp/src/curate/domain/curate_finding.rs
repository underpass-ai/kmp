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
    /// A declared relation whose stated reason Jev doubts, whose type Jev
    /// would choose differently, or that runs the wrong way. Reported only.
    Suspect {
        link: DeclaredLink,
        support: f64,
        best: JevVerdict,
        /// Jev's probability that `from` is the side that holds the
        /// relation; none for a relation that reads the same both ways.
        direction: Option<f64>,
        /// Which of support, type and direction flagged it.
        reasons: Vec<&'static str>,
    },
}
