//! Only the supersessions that touch what a page cites are core; the rest
//! page as expansion, and an answered first page counts them on request.

use std::collections::BTreeSet;

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use serde_json::{Value, json};

use super::projection_outcome::ProjectionOutcome;
use super::recall_output::project_recall_output;
use super::superseded_core::cited_nodes;
use super::test_support::{large_fixture, projected};

const PAGE_BYTES: u64 = 4_000;

/// A cited memory that was replaced, one a citation replaced, and
/// `unrelated` supersessions among memories nothing cites, each with a long
/// `why` as a writer's declared history carries.
fn packet(unrelated: usize, status: Option<&str>) -> Value {
    let mut value = large_fixture(0);
    let mut superseded = vec![
        json!({"ref": "claim:1", "superseded_by": "claim:9", "why": "claim:1 was replaced"}),
        json!({"ref": "claim:old", "superseded_by": "claim:2", "why": "claim:2 replaced it"}),
    ];
    superseded.extend((0..unrelated).map(|index| {
        json!({
            "ref": format!("history:{index:03}"),
            "superseded_by": format!("history:{:03}", index + 1),
            "why": format!("A later statement of attribute {index} replaces the earlier one, as the writer declared.")
        })
    }));
    value["proof"]["superseded"] = Value::Array(superseded);
    if let Some(status) = status {
        value["answer_status"] = json!(status);
    }
    value
}

fn arguments(max_bytes: u64, detail: &str) -> Value {
    json!({"about": "project:kmp", "question": "q", "budget": {"max_bytes": max_bytes, "detail": detail}})
}

fn superseded_refs(value: &Value) -> BTreeSet<String> {
    value
        .pointer("/proof/superseded")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("ref").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

fn touching() -> BTreeSet<String> {
    ["claim:1", "claim:old"].map(ToString::to_string).into()
}

#[test]
fn the_cited_nodes_are_what_core_evidence_supports_and_what_a_detail_id_names() {
    let evidence = [
        json!({"id": "entry:a", "supports": ["node:a"]}),
        json!({"id": "detail:node:b", "supports": []}),
        json!({"id": "entry:c"}),
    ];
    assert_eq!(
        cited_nodes(&evidence),
        ["node:a", "node:b"].map(ToString::to_string).into()
    );
}

#[test]
fn the_core_keeps_only_the_supersessions_that_touch_a_citation() {
    // Compact detail pages no balanced expansion: what remains is the core.
    let value = projected(packet(40, None), arguments(1_000_000, "compact"));
    assert_eq!(superseded_refs(&value), touching());
    let section = &value["projection"]["sections"]["proof.superseded"];
    assert_eq!(section["core"], 2);
    assert_eq!(section["excluded_by_detail"], 40);
}

#[test]
fn a_whole_reading_still_carries_every_supersession() {
    let value = projected(packet(40, None), arguments(1_000_000, "full"));
    assert_eq!(superseded_refs(&value).len(), 42);
    assert_eq!(value["projection"]["page"]["has_more"], false);
}

#[test]
fn a_declared_history_larger_than_the_page_no_longer_overflows_the_core() {
    // Sixty unrelated supersessions are about 7 KB: whole in the core they
    // left no room under 4 KB for the cited proof and one item.
    let bulk = serde_json::to_string(&packet(60, None)["proof"]["superseded"])
        .expect("superseded")
        .len();
    assert!(
        bulk > PAGE_BYTES as usize,
        "fixture history is {bulk} bytes"
    );
    let outcome = project_recall_output(
        packet(60, None),
        &arguments(PAGE_BYTES, "balanced"),
        2_400,
        &Cl100kEstimator::new(),
    )
    .expect("projection");
    let ProjectionOutcome::Projected(value) = outcome else {
        panic!("the cited core fits the page");
    };
    let refs = superseded_refs(&value);
    assert!(refs.is_superset(&touching()));
    assert!(refs.len() < 62, "the page carries only what fits");
    assert!(value["projection"]["page"]["has_more"] == true);
}

#[test]
fn an_answered_first_page_counts_the_rest_of_the_history_on_request() {
    let value = projected(
        packet(60, Some("answered")),
        arguments(PAGE_BYTES, "balanced"),
    );
    let projection = &value["projection"];
    assert_eq!(projection["page"]["has_more"], false);
    assert_eq!(projection["next_action"], Value::Null);
    let carried = superseded_refs(&value).len();
    let more = projection["more_on_request"]
        .as_u64()
        .expect("more_on_request");
    let unrelated_carried = u64::try_from(carried - 2).expect("count");
    assert!(
        more >= 60 - unrelated_carried,
        "every supersession the page left is counted: {more}"
    );
}

#[test]
fn full_detail_pages_the_whole_history_of_an_answered_reading() {
    let mut arguments = arguments(PAGE_BYTES, "full");
    let mut seen = BTreeSet::new();
    let mut first = true;
    loop {
        let page = projected(packet(60, Some("answered")), arguments.clone());
        assert!(page["projection"].get("more_on_request").is_none());
        if first {
            assert!(superseded_refs(&page).is_superset(&touching()));
            first = false;
        }
        seen.extend(superseded_refs(&page));
        let Some(cursor) = page
            .pointer("/projection/page/next_cursor")
            .and_then(Value::as_str)
        else {
            break;
        };
        arguments["page"] = json!({"cursor": cursor});
    }
    assert_eq!(seen.len(), 62);
}

#[test]
fn a_history_that_touches_only_citations_stays_core_and_unreported() {
    let value = projected(
        packet(0, Some("answered")),
        arguments(PAGE_BYTES, "balanced"),
    );
    assert_eq!(superseded_refs(&value), touching());
    assert!(
        value["projection"]["sections"]
            .get("proof.superseded")
            .is_none(),
        "a section without expansion adds nothing to the progress block"
    );
}
