use super::{dropped_identifiers, identifiers, surface_tokens};

#[test]
fn full_named_dates_and_iso_preserve_the_same_calendar_day() {
    for source in [
        "17 de agosto de 2026",
        "August 17, 2026",
        "17 August 2026",
        "2026-08-17",
    ] {
        for rendering in ["2026-08-17", "August 17, 2026", "17 August 2026"] {
            assert!(
                dropped_identifiers(source, rendering).is_empty(),
                "{source} -> {rendering}"
            );
        }
    }
    assert!(dropped_identifiers("el 5 de septiembre de 2026", "on 2026-09-05").is_empty());
}

#[test]
fn changing_or_dropping_any_component_drops_the_whole_date() {
    for rendering in [
        "2026-09-17",
        "2026-08-18",
        "2027-08-17",
        "August 17",
        "2026",
        "17 2026",
    ] {
        assert_eq!(
            dropped_identifiers("17 de agosto de 2026", rendering),
            ["2026-08-17"]
        );
    }
}

#[test]
fn numbers_outside_the_date_cannot_be_hidden_inside_it() {
    assert_eq!(
        dropped_identifiers(
            "El 17 de agosto de 2026 costó 17 EUR (#469, v0.7.0).",
            "On 2026-08-17 the fee was EUR."
        ),
        ["#469", "17", "v0.7.0"]
    );
    assert!(
        dropped_identifiers(
            "17 de agosto de 2026, 17 EUR (#469, v0.7.0)",
            "2026-08-17, 17 EUR (#469, v0.7.0)"
        )
        .is_empty()
    );
}

#[test]
fn unsupported_or_invalid_dates_keep_literal_identifiers() {
    for (source, target) in [
        ("29 de febrero de 2026", "2026-02-29"),
        ("29 de febrero de 1900", "1900-02-29"),
        ("31 de abril de 2026", "2026-04-31"),
        ("0 de agosto de 2026", "2026-08-00"),
        ("17 de agosto de 0000", "0000-08-17"),
        ("17 de agosto", "2026-08-17"),
        ("17/08/2026", "2026-08-17"),
    ] {
        assert!(!dropped_identifiers(source, target).is_empty(), "{source}");
    }
    for year in [2000, 2024] {
        assert!(
            dropped_identifiers(
                &format!("29 de febrero de {year}"),
                &format!("{year}-02-29")
            )
            .is_empty()
        );
    }
}

#[test]
fn date_folding_does_not_change_search_or_stored_tokenization() {
    assert_eq!(
        identifiers("17 de agosto de 2026")
            .into_iter()
            .collect::<Vec<_>>(),
        ["17", "2026"]
    );
    assert!(surface_tokens("August 17, 2026").contains("august"));
    assert_eq!(
        dropped_identifiers("ADR kmp-mcp @rachel #469 09:00 UTC", "Other words"),
        ["#469", "09:00", "@rachel", "adr", "kmp-mcp", "utc"]
    );
}
