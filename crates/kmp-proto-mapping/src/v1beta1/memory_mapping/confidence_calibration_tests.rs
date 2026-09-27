//! P16: a store that asks for a calibration table states `proof.confidence`
//! through it, end to end through `ask_response_from_result`.
use kmp_proto::v1beta1::MemoryConfidence;

use super::anchored_gate_tests::{ask_in, confidence, store_related};
use super::{AskGate, ConfidenceCalibration};

const ENTRIES: &[(&str, &str, &str)] = &[
    (
        "entry-188",
        "decision",
        "Issue #188 pause resume preflight stays open until the recovery lease test passes.",
    ),
    (
        "entry-lease",
        "observation",
        "The recovery lease renewal keeps a stale owner out of the ceremony journal.",
    ),
];

fn table(json: &str) -> &'static ConfidenceCalibration {
    Box::leak(Box::new(ConfidenceCalibration::parse(json).expect("table")))
}

fn ask(calibration: Option<&'static ConfidenceCalibration>, question: &str) -> MemoryConfidence {
    let gate = AskGate::anchored(true).with_confidence_calibration(calibration);
    confidence(&ask_in(Some(gate), question, store_related(ENTRIES, &[])))
}

const ANCHORED: &str = "What is the state of issue #188 pause resume preflight?";
const UNANCHORED: &str = "What keeps a stale owner out of the ceremony journal?";

#[test]
fn without_a_table_confidence_is_what_the_words_earned() {
    assert_eq!(ask(None, ANCHORED), MemoryConfidence::High);
    assert_eq!(ask(None, UNANCHORED), MemoryConfidence::High);
}

#[test]
fn a_rule_that_matches_the_branch_demotes_high_to_medium() {
    let anchored = table(r#"{"version":"t","demote_high":[{"id":"a","branch":"anchored"}]}"#);
    assert_eq!(ask(Some(anchored), ANCHORED), MemoryConfidence::Medium);
    assert_eq!(ask(Some(anchored), UNANCHORED), MemoryConfidence::High);
    let unanchored = table(r#"{"version":"t","demote_high":[{"id":"u","branch":"unanchored"}]}"#);
    assert_eq!(ask(Some(unanchored), ANCHORED), MemoryConfidence::High);
    assert_eq!(ask(Some(unanchored), UNANCHORED), MemoryConfidence::Medium);
}

#[test]
fn integer_thresholds_read_the_concept_counts() {
    // A `high` answer covers at least 60 % of its question: a question of a
    // handful of concepts never misses ten, and always matches fewer than 50.
    let missed = table(r#"{"version":"t","demote_high":[{"id":"m","missed_concepts_above":10}]}"#);
    assert_eq!(ask(Some(missed), ANCHORED), MemoryConfidence::High);
    let thin = table(r#"{"version":"t","demote_high":[{"id":"t","matched_concepts_below":50}]}"#);
    assert_eq!(ask(Some(thin), ANCHORED), MemoryConfidence::Medium);
}

#[test]
fn the_shipped_table_never_raises_a_confidence() {
    let shipped = ConfidenceCalibration::shipped();
    for question in [ANCHORED, UNANCHORED, "What is the capital of Freedonia?"] {
        let plain = ask(None, question);
        let calibrated = ask(Some(shipped), question);
        assert!(
            plain == calibrated
                || (plain == MemoryConfidence::High && calibrated == MemoryConfidence::Medium),
            "{question}: {plain:?} -> {calibrated:?}"
        );
    }
}
