use super::*;
use crate::curate::domain::{
    candidate_pair::CandidatePair, curate_fact::CurateFact, declared_link::DeclaredLink,
    pair_origin::PairOrigin,
};

fn fact(reference: &str, text: &str) -> CurateFact {
    CurateFact {
        reference: reference.into(),
        about: "a".into(),
        kind: String::new(),
        text: text.into(),
        occurred: None,
        labels: Vec::new(),
    }
}

fn link(from: &str, to: &str) -> DeclaredLink {
    DeclaredLink {
        from: from.into(),
        to: to.into(),
        rel: "triggers".into(),
        why: "w".into(),
        evidence: "e".into(),
    }
}

fn pair(from: &str, to: &str) -> CandidatePair {
    CandidatePair {
        from: from.into(),
        to: to.into(),
        origin: PairOrigin::Jev,
        crosses_abouts: false,
    }
}

/// A chain s - a - b - g the graph links, forty unrelated facts, and one
/// unlinked fact that shares the ends' rare words.
fn material() -> CurateMaterial {
    let mut facts = vec![
        fact("s", "The billing outage started after the Valkey failover."),
        fact("a", "Retries piled up on the payments queue."),
        fact("b", "Invoices were sent twice to some customers."),
        fact(
            "g",
            "Refunds were issued for the duplicated invoices after the Valkey outage.",
        ),
        fact(
            "w",
            "Valkey failover and billing outage reviewed in the postmortem.",
        ),
    ];
    facts.extend((0..40).map(|n| fact(&format!("x{n:02}"), &format!("Team lunch number {n}."))));
    CurateMaterial {
        facts,
        declared: vec![link("s", "a")],
        pairs: vec![pair("a", "b"), pair("b", "g")],
        selection: "fp".into(),
        past: Vec::new(),
    }
}

#[test]
fn the_corridor_is_the_balls_around_the_ends_nearest_first() {
    let mut material = material();
    material.pairs.push(pair("g", "x07"));
    material.declared.push(link("x30", "x31"));
    let corridor = Corridor::between(&material, "s", "g");
    let mut linked = corridor.refs[..2].to_vec();
    linked.sort();
    assert_eq!(
        linked,
        ["a", "b"],
        "both lie 3 hops from the two ends: {:?}",
        corridor.refs
    );
    assert_eq!(corridor.refs[0], "b", "the ends' words break the tie");
    assert_eq!(
        corridor.refs[2], "x07",
        "one hop from the goal, four past the start"
    );
    assert_eq!(
        corridor.refs.len(),
        3,
        "facts the balls miss stay out: {:?}",
        corridor.refs
    );
}

#[test]
fn the_corridor_holds_at_most_22_facts_between_the_ends() {
    let mut material = material();
    for n in 0..40 {
        material.pairs.push(pair("s", &format!("x{n:02}")));
    }
    let corridor = Corridor::between(&material, "s", "g");
    assert_eq!(corridor.refs.len(), CORRIDOR_FACTS - 2);
    assert!(!corridor.refs.iter().any(|r| r == "s" || r == "g"));
}

#[test]
fn a_step_is_worth_typing_only_on_a_walk_from_the_start_to_the_goal() {
    let edges = [("s", "a"), ("a", "g"), ("x", "y")];
    let useful = on_some_walk(&edges, "s", "g", 6);
    assert!(useful("s", "a") && useful("g", "a"));
    assert!(!useful("x", "y"), "off every walk");
    assert!(useful("s", "g"), "a direct step");
    let short = on_some_walk(&edges, "s", "g", 1);
    assert!(!short("s", "a"), "the walk through it is too long");
}

#[test]
fn a_selection_the_graph_links_nothing_in_still_gets_a_lexical_corridor() {
    let mut unlinked = material();
    unlinked.declared.clear();
    unlinked.pairs.clear();
    let corridor = Corridor::between(&unlinked, "s", "g");
    assert_eq!(corridor.refs[0], "w", "{:?}", corridor.refs);
    assert_eq!(corridor.refs.len(), CORRIDOR_FACTS - 2);
}

#[test]
fn each_step_offers_its_linked_facts_first_and_at_most_eight() {
    let material = material();
    let walk = ["s", "g", "a", "b", "w"]
        .into_iter()
        .map(str::to_string)
        .chain((0..10).map(|n| format!("x{n:02}")))
        .collect::<Vec<_>>();
    let options = Corridor::step_options(&material, &walk);
    assert_eq!(options.len(), walk.len());
    assert!(
        options
            .values()
            .all(|offered| offered.len() == STEP_OPTIONS)
    );
    assert_eq!(options["a"][..2], ["s".to_string(), "b".to_string()]);
    assert!(
        !options["a"].contains(&"a".to_string()),
        "never its own option"
    );
}
