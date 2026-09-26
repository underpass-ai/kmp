//! Identifier aliases, the `same_entity_as` rescue and what PARTIAL requires,
//! through `ask_response_from_result` as a store that opted into the gate
//! reads them (DISENO §13, decisions of 26 Sept 2026).
use kmp_proto::v1beta1::{AnswerStatus, UnknownReason};

use super::AskGate;
use super::anchored_gate_tests::{ask, ask_in, cited, missing, reason, status, store_related};
use super::morphology::Morphology;
use super::question_contract::QuestionContract;

const CUTS: &[(&str, &str, &str)] = &[
    (
        "decision:corte-10-journal",
        "decision",
        "El corte 10 cerró la migración del journal a formato binario.",
    ),
    (
        "decision:c9-journal",
        "decision",
        "C9 dejó la migración del journal en formato texto.",
    ),
    (
        "decision:adr-018",
        "decision",
        "ADR-018 fixes the journal format as length-prefixed frames.",
    ),
];

#[test]
fn a_guide_word_binds_its_number_to_the_identifier_it_names() {
    let contract = QuestionContract::read("¿Qué cerró el corte 10?", &Morphology::none());
    let anchor = &contract.anchors()[0];
    assert_eq!(anchor.term, "c10");
    assert_eq!(anchor.written, "corte 10");
    assert!(anchor.is_required());

    let contract = QuestionContract::read("What does ADR 18 fix?", &Morphology::none());
    assert_eq!(contract.anchors()[0].term, "adr18");
    let contract = QuestionContract::read("What does ADR-018 fix?", &Morphology::none());
    assert_eq!(contract.anchors()[0].term, "adr18");

    // An issue number is the number `#185` already reads as.
    let contract = QuestionContract::read("What did PR 185 change?", &Morphology::none());
    assert_eq!(contract.anchors()[0].term, "185");
    assert_eq!(contract.asked(), None);
}

#[test]
fn every_spelling_of_a_cut_reaches_the_memory_that_names_it() {
    // The store writes `corte 10`; the reader writes `C10`, `cut 10` or
    // `corte 10`.
    for question in [
        "¿Qué migración del journal cerró C10?",
        "¿Qué migración del journal cerró el cut 10?",
        "¿Qué migración del journal cerró el corte 10?",
    ] {
        let response = ask(question, CUTS);
        assert_eq!(status(&response), AnswerStatus::Answered, "{question}");
        assert_eq!(
            cited(&response),
            ["decision:corte-10-journal"],
            "{question}"
        );
    }
    let response = ask("What journal format does ADR 18 fix?", CUTS);
    assert_eq!(status(&response), AnswerStatus::Answered);
    assert_eq!(cited(&response), ["decision:adr-018"]);
}

#[test]
fn a_cut_nobody_wrote_is_absent_in_the_reader_s_words() {
    let response = ask("¿Qué migración del journal cerró el corte 11?", CUTS);

    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(reason(&response), UnknownReason::AnchorAbsentInSelection);
    assert_eq!(missing(&response), ["corte 11"]);
}

#[test]
fn a_declared_same_entity_answers_for_the_anchor_the_other_names() {
    let entries = [
        (
            "observation:issue-185",
            "observation",
            "Issue #185 reports that resumed ceremonies lose their journal cursor.",
        ),
        (
            "decision:cursor-fix",
            "decision",
            "The resume path now rereads the journal cursor before replaying ceremonies.",
        ),
    ];
    let question = "How was the journal cursor of issue #185 fixed on resume?";

    // Without a declared relation, only the entry that names #185 may be
    // cited, and it says nothing of a fix.
    let alone = ask(question, &entries);
    assert!(
        !cited(&alone).contains(&"decision:cursor-fix"),
        "{:?}",
        cited(&alone)
    );

    let related = store_related(
        &entries,
        &[(
            "decision:cursor-fix",
            "same_entity_as",
            "observation:issue-185",
        )],
    );
    let response = ask_in(Some(AskGate::anchored(true)), question, related);

    assert_eq!(status(&response), AnswerStatus::Answered);
    assert!(cited(&response).contains(&"decision:cursor-fix"));
    let rescued = response
        .proof
        .as_ref()
        .expect("proof")
        .evidence
        .iter()
        .find(|item| item.id.ends_with("decision:cursor-fix"))
        .expect("the rescued memory is in the proof");
    assert_eq!(rescued.metadata["anchor_via"], "same_entity_as");
    assert_eq!(rescued.metadata["anchor_from"], "observation:issue-185");
}

#[test]
fn an_enumeration_that_found_nothing_it_asked_is_unknown_not_partial() {
    let entries = [(
        "decision:c64-adapter",
        "decision",
        "C6.4 local execution adapter runs ceremony steps inside the operator sandbox.",
    )];
    let response = ask(
        "What decisions and evidence are stored for C6.4 about delegation and separation?",
        &entries,
    );

    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
    assert_eq!(missing(&response), ["delegation", "separation"]);
}

#[test]
fn missing_keeps_the_reader_s_accents() {
    let entries = [(
        "decision:c64-adapter",
        "decision",
        "C6.4 local execution adapter runs ceremony steps inside the operator sandbox.",
    )];
    let response = ask("¿Qué delegación registró el C6.4?", &entries);

    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(missing(&response), ["delegación", "registró"]);
}
