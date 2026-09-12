use super::dropped_identifiers;
use crate::{QuestionRendering, SearchSummary};

const SOURCE: &str = "La sesión de formación dura 90 minutos.";

#[test]
fn a_number_kept_in_a_hyphenated_unit_adjective_is_not_dropped() {
    for rendering in [
        "The training session lasts 90 minutes.",
        "This is a 90-minute training session.",
    ] {
        assert!(
            dropped_identifiers(SOURCE, rendering).is_empty(),
            "{rendering}"
        );
    }
}

#[test]
fn a_changed_or_missing_quantity_is_still_dropped() {
    for rendering in [
        "This is a 91-minute training session.",
        "This is a 190-minute training session.",
        "The training session has a fixed duration.",
    ] {
        assert_eq!(
            dropped_identifiers(SOURCE, rendering),
            ["90"],
            "{rendering}"
        );
    }
}

/// The boundary is a number in front of a unit word, and nothing wider. A
/// whole run is read at the joiner, never a substring, so the guarantees the
/// date and ticket comparisons rest on are untouched.
#[test]
fn only_a_number_in_front_of_a_unit_word_is_read_at_the_boundary() {
    assert_eq!(
        dropped_identifiers("costó 17 EUR el 2026-08-17", "it cost EUR on 2026-08-17"),
        ["17"]
    );
    assert_eq!(
        dropped_identifiers("el ticket 42 sigue abierto", "the INC-42 ticket is open"),
        ["42"]
    );
    assert_eq!(dropped_identifiers("90", "a 1990-minute run"), ["90"]);
}

/// Both lints read identifiers through the same comparison, so the rendered
/// question reaches the same boundary the writer's summary does.
#[test]
fn the_summary_and_the_question_read_the_same_boundary() {
    const ASKED: &str = "¿Dura 90 minutos la sesión de formación?";

    SearchSummary::lint(SOURCE, "This is a 90-minute training session.")
        .expect("a faithful summary keeps the number inside the compound");
    QuestionRendering::lint(ASKED, "Is this a 90-minute training session?")
        .expect("the rendered question reads the same boundary");

    assert!(SearchSummary::lint(SOURCE, "This is a 91-minute training session.").is_err());
    assert!(QuestionRendering::lint(ASKED, "Is this a 91-minute training session?").is_err());
}
