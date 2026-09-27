use std::sync::Mutex;

use super::lexical_observation::LexicalObservation;

/// Where an ask leaves what its ranker measured, for the lexical sidecar to
/// be compared with (shadow mode). Given to an ask through
/// `AskRetrievalContext::with_lexical_witness`; the ranker records the first
/// collection it builds and nothing after it. Recording never changes what
/// the ask answers.
#[derive(Debug, Default)]
pub struct LexicalShadowWitness {
    observed: Mutex<Option<LexicalObservation>>,
}

impl LexicalShadowWitness {
    /// Whether nothing has been recorded yet: the ranker skips the work of
    /// observing once something is.
    pub(super) fn is_waiting(&self) -> bool {
        self.observed
            .lock()
            .is_ok_and(|observed| observed.is_none())
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
