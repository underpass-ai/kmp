use std::collections::BTreeSet;

use crate::curate::domain::candidate_pair::CandidatePair;
use crate::curate::domain::curate_fact::CurateFact;
use crate::curate::domain::declared_link::DeclaredLink;

/// One frozen reading to curate: current facts, declared links, candidate
/// pairs, and the fingerprint of the relate selection they came from.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CurateMaterial {
    pub facts: Vec<CurateFact>,
    pub declared: Vec<DeclaredLink>,
    pub pairs: Vec<CandidatePair>,
    pub selection: String,
    /// Facts that are no longer current: never candidates, but the label
    /// values they carry are still the about's catalogue.
    pub past: Vec<CurateFact>,
}

impl CurateMaterial {
    pub(crate) fn fact(&self, reference: &str) -> Option<&CurateFact> {
        self.facts.iter().find(|fact| fact.reference == reference)
    }

    /// Current facts that neither a declared link nor a candidate pair
    /// touches: the ones only a reader of meaning can still pair.
    pub(crate) fn orphans(&self) -> Vec<&CurateFact> {
        let touched = self
            .declared
            .iter()
            .flat_map(|link| [link.from.as_str(), link.to.as_str()])
            .chain(
                self.pairs
                    .iter()
                    .flat_map(|pair| [pair.from.as_str(), pair.to.as_str()]),
            )
            .collect::<BTreeSet<_>>();
        self.facts
            .iter()
            .filter(|fact| !touched.contains(fact.reference.as_str()))
            .collect()
    }
}
