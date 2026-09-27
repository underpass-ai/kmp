use std::collections::BTreeSet;

use super::morphology::Morphology;
use super::search_terms::{fold_search_term, informative_tokens, search_key};

/// The interrogatives that ask for an actor. They are stop words, so the
/// question's own terms no longer carry them; what they ask about is the
/// word that follows.
const ACTOR_INTERROGATIVES: &[&str] = &["who", "whom", "whose", "quien", "quienes"];

/// The attribute a question asks of its topic, when it asks for one: the
/// word after «quién» / «who» (`¿quién aprobó …?` asks who *approved*).
///
/// Behind the store's `attribute_check` (off by default): without an anchor
/// the gate does not decide, and a `high` read from coverage alone can be
/// earned by memories that each say part of the question while none says
/// who did the asked thing (finding `breal-997250f158ea`). With the check, a
/// `high` without an anchor needs a retained citation whose own words state
/// the asked attribute; otherwise it reads `medium`. Status and citations
/// never change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AskedAttribute {
    terms: BTreeSet<String>,
}

impl AskedAttribute {
    /// The attribute `question` asks for, in the ranker's search keys, if
    /// it names an actor interrogative followed by an informative word.
    pub(super) fn read(question: &str, morphology: &Morphology) -> Option<Self> {
        let words = question
            .split(|character: char| !character.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(fold_search_term)
            .collect::<Vec<_>>();
        let mut terms = BTreeSet::new();
        for (index, word) in words.iter().enumerate() {
            if !ACTOR_INTERROGATIVES.contains(&word.as_str()) {
                continue;
            }
            if let Some(next) = words[index + 1..]
                .iter()
                .find_map(|next| informative_tokens(next).next())
            {
                terms.insert(search_key(&next, morphology));
            }
        }
        (!terms.is_empty()).then_some(Self { terms })
    }

    /// The attribute's search keys.
    pub(super) fn terms(&self) -> &BTreeSet<String> {
        &self.terms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_after_the_actor_interrogative_is_the_attribute() {
        let morphology = Morphology::none();
        let spanish = AskedAttribute::read("¿Quién aprobó desplegar el valor?", &morphology)
            .expect("asks for an actor");
        assert_eq!(
            spanish.terms().iter().collect::<Vec<_>>(),
            vec![&"aprobo".to_string()]
        );
        let english =
            AskedAttribute::read("who approved the rollback?", &morphology).expect("an actor");
        assert!(english.terms().contains("approved"));
        assert_eq!(
            AskedAttribute::read("what changed in the rollback?", &morphology),
            None
        );
    }
}
