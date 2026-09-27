use kmp_domain::KmpBundle;
use kmp_domain::language::LanguageVocabulary;

use super::morphology::Morphology;
use super::varint::{Reader, push_unsigned};

/// Function words of each shipped language counted in some memory's texts.
///
/// An about's language is read from all of its texts at once
/// (`search_language`), and the count behind that reading adds over texts.
/// Kept per node and per relation, the counts let the lexical sidecar decide
/// an about's language after a write without reading the about again.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LanguageSignals {
    counts: Vec<u64>,
}

impl LanguageSignals {
    /// The signals of these texts, read the way the ranker reads them.
    pub fn of_texts<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            counts: Morphology::language_signals(texts)
                .into_iter()
                .map(|count| count as u64)
                .collect(),
        }
    }

    /// What `search_language` counts in a whole bundle: node summaries,
    /// details and relation explanations. The tests pin the sidecar's sum
    /// against it.
    pub fn of_bundle(bundle: &KmpBundle) -> Self {
        let mut total = Self::default();
        for node in std::iter::once(bundle.root_node()).chain(bundle.neighbor_nodes()) {
            total.add(&Self::of_texts([node.summary()]));
        }
        for detail in bundle.node_details() {
            total.add(&Self::of_texts([detail.detail()]));
        }
        for relationship in bundle.relationships() {
            total.add(&Self::of_explanation(relationship.explanation()));
        }
        total
    }

    /// The texts of one relation `search_language` reads.
    pub fn of_explanation(explanation: &kmp_domain::RelationExplanation) -> Self {
        Self::of_texts([
            explanation.rationale().unwrap_or_default(),
            explanation.motivation().unwrap_or_default(),
            explanation.evidence().unwrap_or_default(),
        ])
    }

    pub fn is_empty(&self) -> bool {
        self.counts.iter().all(|count| *count == 0)
    }

    pub fn add(&mut self, other: &Self) {
        if self.counts.len() < other.counts.len() {
            self.counts.resize(other.counts.len(), 0);
        }
        for (mine, theirs) in self.counts.iter_mut().zip(&other.counts) {
            *mine += theirs;
        }
    }

    /// Takes away what [`Self::add`] put in. A count cannot go below zero;
    /// an index that tried is out of step and says so.
    pub fn subtract(&mut self, other: &Self) -> Result<(), String> {
        if self.counts.len() < other.counts.len() {
            self.counts.resize(other.counts.len(), 0);
        }
        for (mine, theirs) in self.counts.iter_mut().zip(&other.counts) {
            *mine = mine
                .checked_sub(*theirs)
                .ok_or_else(|| "language signals would go below zero".to_string())?;
        }
        Ok(())
    }

    /// The language these counts read as, by the ranker's own rule.
    pub(super) fn language(&self) -> Option<&'static str> {
        let counts = self
            .counts
            .iter()
            .map(|count| usize::try_from(*count).unwrap_or(usize::MAX))
            .collect::<Vec<_>>();
        LanguageVocabulary::shipped().decide(&counts)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(1 + self.counts.len() * 2);
        push_unsigned(&mut buffer, self.counts.len() as u64);
        for count in &self.counts {
            push_unsigned(&mut buffer, *count);
        }
        buffer
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader::new(bytes);
        let length = reader.unsigned()?;
        let mut counts = Vec::with_capacity(length.min(64) as usize);
        for _ in 0..length {
            counts.push(reader.unsigned()?);
        }
        if !reader.finished() {
            return Err("trailing bytes after language signals".into());
        }
        Ok(Self { counts })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signals_add_over_texts_and_decide_like_the_whole() {
        let texts = [
            "la válvula de reserva se congeló durante la noche",
            "el turno de noche la cambió a las tres",
            "valve",
        ];
        let mut sum = LanguageSignals::default();
        for text in texts {
            sum.add(&LanguageSignals::of_texts([text]));
        }
        assert_eq!(sum, LanguageSignals::of_texts(texts));
        assert_eq!(
            sum.language().map(str::to_string),
            Morphology::read_language(texts)
        );
        assert_eq!(sum.language(), Some("spanish"));
    }

    #[test]
    fn subtracting_what_was_added_returns_to_nothing() {
        let one = LanguageSignals::of_texts(["the valve froze and the crew replaced it"]);
        let mut sum = LanguageSignals::default();
        sum.add(&one);
        sum.subtract(&one).expect("fixture");
        assert!(sum.is_empty());
        assert!(sum.subtract(&one).is_err());
    }

    #[test]
    fn signals_round_trip_through_their_bytes() {
        let signals = LanguageSignals::of_texts(["the valve froze and the crew replaced it"]);
        assert_eq!(
            LanguageSignals::decode(&signals.encode()).expect("fixture"),
            signals
        );
    }
}
