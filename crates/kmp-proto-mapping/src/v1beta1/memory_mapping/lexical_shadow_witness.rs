use std::collections::BTreeSet;
use std::sync::Mutex;

use super::lexical_observation::LexicalObservation;

/// Where an ask leaves what its ranker measured, for the lexical sidecar to
/// be compared with (shadow mode). Given to an ask through
/// `AskRetrievalContext::with_lexical_witness`; the ranker records the first
/// collection it builds and nothing after it. Recording never changes what
/// the ask answers.
///
/// It also records which candidates the first ranking found eligible, for
/// `verify` to check the lexical index set none of them aside (P14). A
/// witness made with [`Self::eligibility`] records only that.
#[derive(Debug, Default)]
pub struct LexicalShadowWitness {
    observed: Mutex<Option<LexicalObservation>>,
    eligible: Mutex<Option<BTreeSet<String>>>,
    eligibility_only: bool,
}

impl LexicalShadowWitness {
    /// A witness that records only which candidates are eligible.
    pub fn eligibility() -> Self {
        Self {
            eligibility_only: true,
            ..Self::default()
        }
    }

    /// Whether nothing has been recorded yet: the ranker skips the work of
    /// observing once something is.
    pub(super) fn is_waiting(&self) -> bool {
        !self.eligibility_only
            && self
                .observed
                .lock()
                .is_ok_and(|observed| observed.is_none())
    }

    /// Records the candidates the first ranking found eligible.
    pub(super) fn record_eligible(&self, eligible: impl Iterator<Item = String>) {
        if let Ok(mut held) = self.eligible.lock()
            && held.is_none()
        {
            *held = Some(eligible.collect());
        }
    }

    /// Every candidate the first ranking found eligible, taken once.
    pub fn take_eligible(&self) -> Option<BTreeSet<String>> {
        self.eligible.lock().ok().and_then(|mut held| held.take())
    }

    pub(super) fn record(&self, observation: LexicalObservation) {
        if let Ok(mut observed) = self.observed.lock()
            && observed.is_none()
        {
            *observed = Some(observation);
        }
    }

    /// What the ask's ranker measured, taken once.
    pub fn take(&self) -> Option<LexicalObservation> {
        self.observed
            .lock()
            .ok()
            .and_then(|mut observed| observed.take())
    }
}
