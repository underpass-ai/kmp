use super::dropped_identifiers;
use crate::{QuestionRendering, SearchSummary};

const SOURCE: &str = "El error de S-4 fue 0,6 °C.";

#[test]
fn a_decimal_comma_is_carried_by_the_point_english_writes() {
    for rendering in [
        "The error of S-4 was 0.6 °C.",
        "The error of S-4 was 0,6 °C.",
    ] {
        assert!(
            dropped_identifiers(SOURCE, rendering).is_empty(),
            "{rendering}"
        );
    }
    for (source, rendering) in [("1,0", "1.0"), ("1,4", "1.4"), ("12,45", "12.45")] {
        assert!(
            dropped_identifiers(source, rendering).is_empty(),
            "{source} -> {rendering}"
        );
    }
}

#[test]
fn a_changed_value_or_a_changed_identifier_is_still_dropped() {
    assert_eq!(
        dropped_identifiers(SOURCE, "The error of S-4 was 0.7 °C."),
        ["0,6"]
    );
    assert_eq!(
        dropped_identifiers(SOURCE, "The error of S-5 was 0.6 °C."),
        ["s-4"]
    );
    // Trailing zeros are a different spelling of the same quantity. Reading
    // that needs a parser, so the digits stay literal and the pair is refused.
    assert_eq!(dropped_identifiers("0,60", "0.6"), ["0,60"]);
    assert_eq!(dropped_identifiers("0,6", "6"), ["0,6"]);
}

/// A sign belongs to the number, and one quantity never covers another.
#[test]
fn signs_are_kept_and_independent_quantities_stay_independent() {
    assert!(dropped_identifiers("-0,6", "-0.6").is_empty());
    assert!(dropped_identifiers("+1,4", "+1.4").is_empty());
    assert_eq!(dropped_identifiers("-0,6", "0.6"), ["-0,6"]);
    assert_eq!(
        dropped_identifiers("El sesgo fue 0,6 y el límite 0,8.", "The bias was 0.6."),
        ["0,8"]
    );
}

/// `1,500` is one thousand five hundred to one writer and one and a half to
/// another. The token cannot say which, so the limitation is kept rather than
/// guessed at and both spellings stay distinct.
#[test]
fn an_ambiguous_thousands_group_is_not_read_as_a_decimal() {
    assert_eq!(dropped_identifiers("1,500", "1.500"), ["1,500"]);
    assert_eq!(dropped_identifiers("1.500", "1,500"), ["1.500"]);
    assert_eq!(dropped_identifiers("1,500", "1500"), ["1,500"]);
    assert_eq!(dropped_identifiers("12,345", "12.345"), ["12,345"]);
    assert_eq!(dropped_identifiers("1,234,567", "1.234.567"), ["1,234,567"]);
}

#[test]
fn dates_versions_tickets_times_and_paths_are_untouched() {
    for (source, rendering) in [
        ("v0.7.0", "v0,7,0"),
        ("2026-08-17", "2026,08,17"),
        ("09:00", "09,00"),
        ("#469", "#4,69"),
        ("ref_boundary.rs", "ref_boundary,rs"),
    ] {
        assert!(
            !dropped_identifiers(source, rendering).is_empty(),
            "{source} -> {rendering}"
        );
    }
    // The date comparison still reads a calendar day, not a separator.
    assert!(dropped_identifiers("17 de agosto de 2026", "2026-08-17").is_empty());
}

/// Both lints read identifiers through the same comparison.
#[test]
fn the_summary_and_the_question_read_the_same_boundary() {
    SearchSummary::lint(SOURCE, "The error of S-4 was 0.6 °C.")
        .expect("a faithful summary may write the decimal point");
    QuestionRendering::lint(
        "¿Cuál fue el error de S-4, 0,6 °C?",
        "Was the S-4 error 0.6 °C?",
    )
    .expect("the rendered question reads the same boundary");

    assert!(SearchSummary::lint(SOURCE, "The error of S-4 was 0.7 °C.").is_err());
}
