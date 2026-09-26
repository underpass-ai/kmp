use std::collections::BTreeSet;
use std::sync::OnceLock;

use serde::Deserialize;

use super::morphology::Morphology;
use super::question_vocabulary::QuestionVocabulary;
use super::search_terms::{informative_terms, informative_tokens, query_requests_lifecycle};

const SOURCE: &str = include_str!("../../../language/lifecycle_questions.json");

/// What a question asks of a declared lifecycle.
///
/// A question about now wants the current head of whatever it matched that
/// was replaced; a question about history wants the chain. History is asked
/// by a word of the lifecycle family (`replaced`, `previous`, `cambio`), a
/// history word (`historial`, `antes`, `timeline`), the lifecycle words the
/// ranker already lets reach replaced memories (`before`, `old`), or by a
/// selection that stands at an instant (`as_of`, `interval`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LifecycleAsk {
    Current,
    History,
}

#[derive(Debug, Deserialize)]
struct HistoryWords {
    history_words: BTreeSet<String>,
}

impl LifecycleAsk {
    pub(super) fn read(question: &str, morphology: &Morphology, at_an_instant: bool) -> Self {
        if at_an_instant || asks_history(question, morphology) {
            Self::History
        } else {
            Self::Current
        }
    }
}

fn asks_history(question: &str, morphology: &Morphology) -> bool {
    let families = QuestionVocabulary::shipped();
    let words = &history_words().history_words;
    informative_tokens(question)
        .any(|token| words.contains(&token) || families.family_of(&token) == Some("lifecycle"))
        || query_requests_lifecycle(&informative_terms(question, morphology), morphology)
}

fn history_words() -> &'static HistoryWords {
    static SHIPPED: OnceLock<HistoryWords> = OnceLock::new();
    SHIPPED.get_or_init(|| serde_json::from_str(SOURCE).expect("the shipped history words parse"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(question: &str) -> LifecycleAsk {
        LifecycleAsk::read(question, &Morphology::for_language(Some("english")), false)
    }

    #[test]
    fn a_question_about_now_asks_for_the_current_head() {
        assert_eq!(
            read("Which message bus does pool-003 run on now?"),
            LifecycleAsk::Current
        );
        assert_eq!(
            read("What is the capital of Freedonia?"),
            LifecycleAsk::Current
        );
    }

    #[test]
    fn history_is_asked_in_either_language_or_by_the_lifecycle_family() {
        for question in [
            "Show the history of the pool-003 broker.",
            "¿Cuál es el historial del broker de pool-003?",
            "¿Qué broker usaba pool-003 antes?",
            "Which broker replaced RabbitMQ for pool-003?",
            "What was the previous broker of pool-003?",
        ] {
            assert_eq!(read(question), LifecycleAsk::History, "{question}");
        }
    }

    #[test]
    fn a_selection_at_an_instant_is_a_history_question() {
        assert_eq!(
            LifecycleAsk::read(
                "Which message bus does pool-003 run on?",
                &Morphology::for_language(Some("english")),
                true
            ),
            LifecycleAsk::History
        );
    }
}
