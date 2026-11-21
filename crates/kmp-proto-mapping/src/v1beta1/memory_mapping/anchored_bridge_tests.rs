//! The anchored gate with a lexical-bridge table, through
//! `ask_response_from_result` as a store reads it: the table translates a
//! concept across a language and never stands in for one inside a language.
use kmp_application::MemoryAnswerPolicy;
use kmp_domain::TemporalSelection;
use kmp_proto::v1beta1::{AnswerStatus, AskResponse, MemoryConfidence, UnknownReason};

use super::anchored_gate_tests::{confidence, reason, status, store_related};
use super::lexical_bridge::tests::table;
use super::{AskGate, AskRetrievalContext, LexicalBridge, ask_response_from_result};

/// `deadline`, its Spanish `plazo` and the English neighbour `final` point
/// the same way; `prefix` and `prefer`, `deliberate` and `delivered` lean
/// together.
fn toy() -> LexicalBridge {
    LexicalBridge::from_bytes(&table(
        "toy",
        &[
            ("deadline", &[127, 0, 0, 0]),
            ("plazo", &[120, 30, 0, 0]),
            ("final", &[110, 0, 50, 0]),
            ("prefix", &[0, 127, 0, 0]),
            ("prefer", &[0, 110, 0, 50]),
            ("deliberate", &[0, 0, 127, 0]),
            ("delivered", &[0, 0, 120, 40]),
            ("retention", &[0, 0, 0, 127]),
        ],
    ))
    .expect("the toy table parses")
}

fn ask(bridge: &LexicalBridge, question: &str, entries: &[(&str, &str, &str)]) -> AskResponse {
    ask_response_from_result(
        question,
        None,
        MemoryAnswerPolicy::EvidenceOrUnknown,
        None,
        AskRetrievalContext::from(store_related(entries, &[])).with_gate(AskGate::anchored(true)),
        bridge,
        &TemporalSelection::Frontier,
    )
    .expect("an ask response")
}

#[test]
fn a_neighbour_in_the_same_language_does_not_state_the_attribute() {
    let question = "What deadline does #10132 set for svc-013?";
    let entries = [(
        "decision:retention",
        "decision",
        "#10132 sets the log retention of svc-013 to 14 days. The final paragraph mentions karu.",
    )];
    for bridge in [toy(), LexicalBridge::none()] {
        let response = ask(&bridge, question, &entries);
        assert_eq!(status(&response), AnswerStatus::Unknown, "{response:?}");
        assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
        assert!(response.because.is_empty());
    }
}

#[test]
fn a_prefix_neighbour_is_not_the_word_the_question_asked() {
    let entries = [(
        "observation:prefer",
        "observation",
        "The kernel prefer mode #7 delivered a decision from another workflow.",
    )];
    let response = ask(
        &toy(),
        "Was kernel prefix #7 a deliberate decision?",
        &entries,
    );
    assert_eq!(status(&response), AnswerStatus::Unknown, "{response:?}");
}

#[test]
fn a_concept_translated_across_a_language_answers_at_medium_at_most() {
    let question = "What is the deadline of #10132 for svc-013 in eu-west-1?";
    let entries = [(
        "decision:plazo",
        "decision",
        "#10132 fija el plazo de svc-013 en eu-west-1 para el viernes.",
    )];

    let translated = ask(&toy(), question, &entries);
    assert_eq!(
        status(&translated),
        AnswerStatus::Answered,
        "{translated:?}"
    );
    assert_eq!(translated.because[0].claim, "decision:plazo");
    // The anchors match in the reader's words and would read `high`; the
    // attribute stands only through the table.
    assert_eq!(confidence(&translated), MemoryConfidence::Medium);

    let unbridged = ask(&LexicalBridge::none(), question, &entries);
    assert_eq!(status(&unbridged), AnswerStatus::Unknown);
    assert_eq!(reason(&unbridged), UnknownReason::AttributeNotFound);
}

#[test]
fn a_concept_stated_in_the_readers_words_keeps_its_confidence() {
    let question = "What is the deadline of #10132 for svc-013 in eu-west-1?";
    let entries = [(
        "decision:deadline",
        "decision",
        "#10132 moves the deadline of svc-013 in eu-west-1 to Friday.",
    )];
    for bridge in [toy(), LexicalBridge::none()] {
        let response = ask(&bridge, question, &entries);
        assert_eq!(status(&response), AnswerStatus::Answered, "{response:?}");
        assert_eq!(confidence(&response), MemoryConfidence::High);
    }
}
