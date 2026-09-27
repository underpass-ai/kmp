//! Notation a faithful rendering says in words: an approximate number, an
//! arrow, a slashed list of words and, in a Spanish text, thousands grouped
//! with a dot. Each is accepted in its translated form, and each still
//! requires what it carries: the number, the sides of the arrow, a path.

use super::{dropped_identifiers, required_identifiers};
use crate::SearchSummary;

#[test]
fn an_approximate_number_is_carried_by_about_or_approximately() {
    let source = "La cola tenía ~300 mensajes pendientes.";
    for rendering in [
        "The queue held about 300 pending messages.",
        "The queue held approximately 300 pending messages.",
        "The queue held ~300 pending messages.",
        "The queue held ≈300 pending messages.",
    ] {
        assert!(
            dropped_identifiers(source, rendering).is_empty(),
            "{rendering}"
        );
    }
    // The number itself is still required, and a changed one is refused.
    assert_eq!(
        dropped_identifiers(source, "The queue held a few hundred messages."),
        ["300"]
    );
    assert_eq!(
        dropped_identifiers(source, "The queue held about 30 messages."),
        ["300"]
    );
    // A tilde in front of anything but a digit is not an approximation.
    assert_eq!(
        dropped_identifiers(
            "Se fijó el alias ~v2 en el perfil.",
            "The alias v2 was pinned."
        ),
        ["~v2"]
    );
}

#[test]
fn an_arrow_is_carried_by_its_sides_in_words_or_as_ascii() {
    let source = "El total bajó 36347→23046 tras deduplicar.";
    for rendering in [
        "The total fell 36347 to 23046 after deduplication.",
        "The total fell 36347->23046 after deduplication.",
        "The total fell from 36347 to 23046 after deduplication.",
        "The total fell 36347→23046 after deduplication.",
    ] {
        assert!(
            dropped_identifiers(source, rendering).is_empty(),
            "{rendering}"
        );
    }
    // Each side is still a number the rendering must keep.
    assert_eq!(
        dropped_identifiers(source, "The total fell to 23046 after deduplication."),
        ["36347"]
    );
    // Single-digit sides were never identifiers on their own.
    assert!(
        dropped_identifiers("Pasó de 1→6 réplicas.", "It went from 1 to 6 replicas.").is_empty()
    );
}

#[test]
fn a_slashed_list_of_words_is_translated_like_a_pair() {
    for (source, rendering) in [
        (
            "Estados vivos/expirados/retirados en la proyección.",
            "Live, expired and retired states in the projection.",
        ),
        (
            "Se añadió pausa/reanudación/cancelación al ciclo.",
            "Pause/resume/cancel was added to the lifecycle.",
        ),
        (
            "Paridad grpc/mcp/embedded verificada.",
            "Parity across gRPC, MCP and embedded verified.",
        ),
        ("Salud/escudo se recuperan.", "Health and shields recover."),
    ] {
        assert!(
            dropped_identifiers(source, rendering).is_empty(),
            "{source} -> {rendering}"
        );
    }
}

#[test]
fn a_slashed_list_that_reads_as_a_path_or_a_name_is_still_copied() {
    for (source, dropped) in [
        ("Rama feat/valkey/store-v2 creada.", "feat/valkey/store-v2"),
        (
            "Fichero crates/kmp/src/lib.rs editado.",
            "crates/kmp/src/lib.rs",
        ),
        ("Ruta /home/ana/docs vacía.", "/home/ana/docs"),
        ("Ruta docs/notas/viejas/ vacía.", "docs/notas/viejas/"),
        ("Estados PASS/FAIL/SKIP registrados.", "pass/fail/skip"),
        ("Fases c1/c2/c3 cerradas.", "c1/c2/c3"),
    ] {
        assert_eq!(
            dropped_identifiers(source, "Something else entirely happened."),
            [dropped],
            "{source}"
        );
    }
}

#[test]
fn git_hashes_and_long_paths_stay_mandatory() {
    let source = "Fusionado como 51195954b8a9d526e026b32e8c1de0ee3fd1b659 en \
                  /home/gx10a/Documents/ai/artifacts/made/handoffs/handoff-c3.md.";
    assert_eq!(
        dropped_identifiers(source, "Merged into the handoff directory."),
        [
            "/home/gx10a/documents/ai/artifacts/made/handoffs/handoff-c3.md",
            "51195954b8a9d526e026b32e8c1de0ee3fd1b659"
        ]
    );
    assert_eq!(
        dropped_identifiers("Fusionado en bbfb06d.", "Merged."),
        ["bbfb06d"]
    );
}

#[test]
fn spanish_thousands_with_a_dot_are_carried_by_the_english_integer() {
    let source = "Se procesaron 50.976 filas del lote.";
    for rendering in [
        "50,976 rows of the batch were processed.",
        "50976 rows of the batch were processed.",
        "50.976 rows of the batch were processed.",
    ] {
        assert!(
            dropped_identifiers(source, rendering).is_empty(),
            "{rendering}"
        );
    }
    assert!(
        dropped_identifiers(
            "El índice tiene 1.234.567 términos.",
            "The index holds 1,234,567 terms."
        )
        .is_empty()
    );
    // A different integer, or a decimal reading of the same digits, is not it.
    for rendering in [
        "50,977 rows of the batch were processed.",
        "5,097 rows of the batch were processed.",
        "50.9 rows of the batch were processed.",
    ] {
        assert_eq!(
            dropped_identifiers(source, rendering),
            ["50.976"],
            "{rendering}"
        );
    }
}

/// The ambiguous cases the language does not settle keep the literal reading.
#[test]
fn thousands_the_language_cannot_settle_stay_literal() {
    // An English text's `50.976` is a decimal to its writer.
    assert_eq!(
        dropped_identifiers(
            "The ratio reached 50.976 at peak.",
            "The ratio reached 50,976 at peak."
        ),
        ["50.976"]
    );
    // A leading zero is never a grouping, even in Spanish.
    assert_eq!(
        dropped_identifiers(
            "La precisión fue 0.976 en la prueba.",
            "Precision was 976 in the test."
        ),
        ["0.976"]
    );
    // A comma in the Spanish text is its decimal separator: an integer is
    // not that decimal.
    assert_eq!(
        dropped_identifiers(
            "El ratio fue 50,976 en la prueba.",
            "The ratio was 50976 in the test."
        ),
        ["50,976"]
    );
    // A text whose language cannot be read keeps the literal comparison.
    assert_eq!(dropped_identifiers("50.976", "50,976"), ["50.976"]);
    // Versions keep every separator.
    assert_eq!(
        dropped_identifiers(
            "Se publicó la versión 1.500.2 del cliente.",
            "Client version 1,500,2 was published."
        ),
        ["1.500.2"]
    );
}

#[test]
fn the_required_identifiers_are_named_as_the_text_writes_them() {
    assert_eq!(
        required_identifiers(
            "Se fusionó la PR #821 en kmp-viewer (ADR-018), con ~300 cambios y 36347→23046 filas."
        ),
        [
            "PR",
            "#821",
            "kmp-viewer",
            "ADR-018",
            "300",
            "36347",
            "23046"
        ]
    );
    assert_eq!(
        SearchSummary::required_identifiers("La válvula se congeló."),
        Vec::<String>::new()
    );
    // A date spelled in words is named in the canonical form the lint compares.
    assert_eq!(
        required_identifiers("El 17 de agosto de 2026 se abrió la ruta."),
        ["2026-08-17"]
    );
}

#[test]
fn a_spanish_decimal_comma_is_carried_by_the_english_point() {
    let source = "El ratio fue 50,976 en la prueba.";
    for rendering in [
        "The ratio was 50.976 in the test.",
        "The ratio was 50,976 in the test.",
    ] {
        assert!(
            dropped_identifiers(source, rendering).is_empty(),
            "{rendering}"
        );
    }
    assert!(
        dropped_identifiers(
            "La precisión fue 0,976 en la prueba.",
            "Precision was 0.976 in the test."
        )
        .is_empty()
    );
    assert!(
        dropped_identifiers(
            "El sesgo fue -1,500 en la prueba.",
            "The bias was -1.500 in the test."
        )
        .is_empty()
    );
}

/// What the language does not settle, or what is not one decimal, stays literal.
#[test]
fn a_comma_the_language_cannot_settle_stays_literal() {
    // An English text's `50,976` is an integer to its writer.
    assert_eq!(
        dropped_identifiers(
            "The table held 50,976 rows at peak.",
            "The table held 50.976 rows at peak."
        ),
        ["50,976"]
    );
    // Several commas are thousands groups, never one decimal.
    assert_eq!(
        dropped_identifiers(
            "El índice tiene 1,234,567 términos.",
            "The index holds 1.234.567 terms."
        ),
        ["1,234,567"]
    );
    // A changed decimal is refused.
    assert_eq!(
        dropped_identifiers(
            "El ratio fue 50,976 en la prueba.",
            "The ratio was 50.97 in the test."
        ),
        ["50,976"]
    );
    // An undecided text keeps the literal comparison.
    assert_eq!(dropped_identifiers("50,976", "50.976"), ["50,976"]);
}

#[test]
fn a_slashed_list_shaped_like_a_code_path_is_copied_even_without_extension() {
    for (source, path) in [
        (
            "Se tocó src/write/planner para el pase único.",
            "src/write/planner",
        ),
        (
            "Los tests viven en crates/kmp/tests desde ayer.",
            "crates/kmp/tests",
        ),
        (
            "La guía está en docs/development ahora.",
            "docs/development",
        ),
        ("El script scripts/ci/gates falla.", "scripts/ci/gates"),
    ] {
        assert_eq!(
            dropped_identifiers(source, "Something changed in the planner."),
            [path],
            "{source}"
        );
        let rendering = format!("It changed in {path}.");
        assert!(
            dropped_identifiers(source, &rendering).is_empty(),
            "{source}"
        );
    }
    // Prose lists name no source directory and stay translatable, even when
    // one of their words is a directory name in another case.
    for (source, rendering) in [
        (
            "Estados vivos/expirados/retirados.",
            "Live, expired and retired states.",
        ),
        (
            "Entradas input/output revisadas.",
            "Input and output entries reviewed.",
        ),
        (
            "Pruebas Tests/Docs/Ejemplos revisadas.",
            "Tests, docs and examples reviewed.",
        ),
    ] {
        assert!(
            dropped_identifiers(source, rendering).is_empty(),
            "{source}"
        );
    }
}

#[test]
fn acronyms_in_a_slashed_list_stay_mandatory() {
    assert_eq!(
        dropped_identifiers(
            "Paridad gRPC/MCP/embedded verificada.",
            "Parity across gRPC, MCP and embedded verified."
        ),
        ["grpc/mcp/embedded"]
    );
    assert_eq!(
        dropped_identifiers("Estado PASS/FAIL anotado.", "Pass or fail status noted."),
        ["pass/fail"]
    );
}
