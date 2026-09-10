use super::{dropped_identifiers, identifiers, surface_tokens};
use crate::{QuestionRendering, SearchSummary};

#[test]
fn possessives_carry_whole_identifiers_in_either_rendering_direction() {
    for identifier in [
        "O2", "R2", "P9", "ADR", "v0.7.0", "#660", "@rachel", "kmp-mcp",
    ] {
        for suffix in ["'s", "’s", "'S", "’S"] {
            let possessive = format!("({identifier}{suffix}),");
            assert!(dropped_identifiers(identifier, &possessive).is_empty());
            assert!(dropped_identifiers(&possessive, identifier).is_empty());
            assert!(!dropped_identifiers(&possessive, "something else").is_empty());
        }
    }
}

#[test]
fn possessive_matching_does_not_accept_substrings_or_changed_literals() {
    for (source, replacements) in [
        ("O2", &["O3", "O20", "XO2", "O2s", "O2.s"][..]),
        ("72", &["73", "172"][..]),
        ("v0.7.0", &["v0.7.1", "v0.7.00"][..]),
        ("#660", &["#661", "#6600"][..]),
    ] {
        for replacement in replacements {
            for suffix in ["", "'s", "’s"] {
                assert_eq!(
                    dropped_identifiers(source, &format!("{replacement}{suffix}")),
                    [source.to_lowercase()]
                );
            }
        }
    }
}

#[test]
fn summary_and_question_fidelity_preserve_the_supplied_rendering() {
    let source = "R7 utiliza el plan O2.";
    for rendering in [
        "R7 uses the plan of O2.",
        "R7 uses O2's plan.",
        "R7 uses O2’s plan.",
    ] {
        assert_eq!(
            SearchSummary::lint(source, rendering)
                .expect("faithful summary")
                .as_str(),
            rendering
        );
        assert_eq!(
            QuestionRendering::lint(source, rendering)
                .expect("faithful question")
                .as_str(),
            rendering
        );
    }
    assert!(SearchSummary::lint(source, "R7 uses O3's plan.").is_err());
    assert!(QuestionRendering::lint(source, "R7 uses O3’s plan.").is_err());
}

#[test]
fn possessive_fidelity_does_not_change_public_search_tokenization() {
    assert!(identifiers("O2's plan").contains("o2's"));
    assert!(surface_tokens("O2’s plan").contains("o2’s"));
    assert!(!surface_tokens("O2's plan").contains("o2"));
}
