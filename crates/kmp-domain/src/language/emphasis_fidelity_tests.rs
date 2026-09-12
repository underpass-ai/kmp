use super::{dropped_identifiers, identifiers};
use crate::SearchSummary;

#[test]
fn a_shouted_phrase_is_emphasis_and_not_a_run_of_acronyms() {
    let source = "BLOQUEANTE ANTES DE MIGRAR NADA: la sonda se cayó.";

    assert!(identifiers(source).is_empty(), "{:?}", identifiers(source));
    assert!(
        dropped_identifiers(
            source,
            "Blocking before migrating anything: the probe went down."
        )
        .is_empty()
    );
}

#[test]
fn an_acronym_inside_ordinary_prose_is_still_copied() {
    let source = "El control QA terminó en PASS.";

    assert_eq!(
        identifiers(source).into_iter().collect::<Vec<_>>(),
        ["pass", "qa"]
    );
    assert_eq!(
        dropped_identifiers(source, "The check finished."),
        ["pass", "qa"]
    );
}

/// Shape alone cannot separate `PASS` from `PERO`. The shipped function words
/// are the one dictionary that can, and they read the same on every machine.
#[test]
fn a_shouted_function_word_is_that_word_and_not_an_abbreviation() {
    let source = "Funciona, PERO la sonda falla.";

    assert!(!identifiers(source).contains("pero"));
    assert!(dropped_identifiers(source, "It works, but the probe fails.").is_empty());
}

#[test]
fn a_slashed_pair_of_ordinary_words_is_prose_a_rendering_translates() {
    for source in [
        "El manual explica las entradas/salidas del sistema.",
        "The manual explains the cleanup/binding order.",
        "The manual explains the input/output order.",
    ] {
        assert!(identifiers(source).is_empty(), "{source}");
    }

    assert!(
        dropped_identifiers(
            "El manual explica las entradas/salidas del sistema.",
            "The manual explains the system inputs and outputs.",
        )
        .is_empty()
    );
}

#[test]
fn a_path_a_branch_or_a_shouted_pair_is_still_copied() {
    let found = identifiers("feat/valkey-store, src/lib.rs, PASS/FAIL, ADR-018");

    assert_eq!(
        found.into_iter().collect::<Vec<_>>(),
        ["adr-018", "feat/valkey-store", "pass/fail", "src/lib.rs"]
    );
}

/// The refusals this covers were answered by pasting the Spanish back into the
/// field that exists to carry English. The English rendering now carries.
#[test]
fn the_writer_lint_accepts_the_english_it_asked_for() {
    SearchSummary::lint(
        "El manual explica las entradas/salidas del sistema.",
        "The manual explains the system inputs and outputs.",
    )
    .expect("a slashed pair of words is translated, not copied");

    SearchSummary::lint(
        "BLOQUEANTE ANTES DE MIGRAR NADA, commit a4a8b74.",
        "Blocking before migrating anything, commit a4a8b74.",
    )
    .expect("emphasis is translated while the commit is copied");
}
