use super::dropped_identifiers;

#[test]
fn partial_named_dates_preserve_known_components_without_inventing_a_year() {
    for source in ["18 de agosto", "18 August", "August 18"] {
        for rendering in ["August 18", "18 August", "2026-08-18", "18 August 2026"] {
            assert!(
                dropped_identifiers(source, rendering).is_empty(),
                "{source} -> {rendering}"
            );
        }
    }
    // The comparator can check only the source's month and day. A year
    // supplied by the writer is not a year inferred or verified by the lint.
    assert_eq!(
        super::date_tokens::canonical_dates(vec!["18".into(), "August".to_lowercase()]),
        ["--08-18"]
    );
    assert!(dropped_identifiers("29 de febrero", "February 29").is_empty());
}

#[test]
fn a_partial_date_cannot_be_replaced_by_a_bare_number_or_another_month_or_day() {
    for rendering in [
        "2026-09-18",
        "2026-08-19",
        "September 18",
        "August 19",
        "18",
    ] {
        assert_eq!(dropped_identifiers("18 de agosto", rendering), ["--08-18"]);
    }
    assert_eq!(
        dropped_identifiers("18 de agosto de 2026", "August 18"),
        ["2026-08-18"]
    );
    assert!(!dropped_identifiers("29 de febrero", "2026-02-29").is_empty());
    assert!(!dropped_identifiers("29 de febrero de 2026", "2024-02-29").is_empty());
}

#[test]
fn partial_dates_do_not_cover_independent_amounts_or_tickets() {
    let source = "INC-42 llegó el 18 de agosto; coste 18 EUR (#647).";
    assert_eq!(
        dropped_identifiers(source, "INC-42 arrived on 2026-08-18; fee in EUR (#647)."),
        ["18"]
    );
    assert_eq!(
        dropped_identifiers(source, "It arrived on August 18; fee 18 EUR (#647)."),
        ["inc-42"]
    );
    assert!(
        dropped_identifiers(source, "INC-42 arrived on August 18; fee 18 EUR (#647).").is_empty()
    );
}
