//! Metric parity between the Rust scorecard and the Python memory bench.
//!
//! `judged/metric_parity.json` holds ranking outcomes and the numbers they
//! must score. `scripts/performance/memory_bench/tests/test_metrics.py` reads
//! the same file, so a change to either implementation that moves a number
//! fails on both sides until the fixture is regenerated on purpose. Its
//! `refs` table does the same for `memory_ref::normalize` and `refs.normalize`.
use kmp_testkit::memory_ref::{normalize, retrieved};
use kmp_testkit::retrieval_scorecard::{RetrievalOutcome, RetrievalScorecard};
use serde_json::Value;

const FIXTURE: &str = include_str!("../judged/metric_parity.json");

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("array of refs")
        .iter()
        .map(|item| item.as_str().expect("ref string").to_string())
        .collect()
}

fn outcome(case: &Value) -> RetrievalOutcome {
    RetrievalOutcome {
        judged: strings(&case["judged"]).into_iter().collect(),
        retrieved: strings(&case["retrieved"]),
        cited: strings(&case["cited"]).into_iter().collect(),
        unknown: case["unknown"].as_bool().expect("unknown flag"),
        used_bytes: case["used_bytes"].as_u64().expect("used_bytes"),
        elapsed_millis: case["elapsed_millis"].as_u64().expect("elapsed_millis"),
    }
}

fn assert_close(name: &str, what: &str, actual: f64, expected: &Value, tolerance: f64) {
    let expected = expected.as_f64().expect("expected number");
    assert!(
        (actual - expected).abs() <= tolerance,
        "{name}: {what} = {actual}, fixture says {expected}"
    );
}

#[test]
fn every_case_scores_what_the_shared_fixture_says() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture is JSON");
    let tolerance = fixture["tolerance"].as_f64().expect("tolerance");
    let cases = fixture["cases"].as_array().expect("cases");
    assert!(!cases.is_empty());
    for case in cases {
        let name = case["name"].as_str().expect("case name");
        let expected = &case["expected"];
        let scored = outcome(case);
        for (what, actual) in [
            ("recall_at_1", scored.recall_at(1)),
            ("recall_at_5", scored.recall_at(5)),
            ("recall_at_10", scored.recall_at(10)),
            ("reciprocal_rank", scored.reciprocal_rank()),
            ("ndcg_at_10", scored.ndcg_at(10)),
        ] {
            assert_close(name, what, actual, &expected[what], tolerance);
        }
        for (what, actual) in [
            ("answer_cites_judged", scored.answer_cites_judged()),
            ("is_false_unknown", scored.is_false_unknown()),
            (
                "has_complete_support_at_5",
                scored.has_complete_support_at(5),
            ),
        ] {
            assert_eq!(Some(actual), expected[what].as_bool(), "{name}: {what}");
        }
    }
}

#[test]
fn the_collection_aggregates_to_the_shared_scorecard() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture is JSON");
    let tolerance = fixture["tolerance"].as_f64().expect("tolerance");
    let outcomes = fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(outcome)
        .collect::<Vec<_>>();
    let card = RetrievalScorecard::score(&outcomes);
    let expected = &fixture["scorecard"];

    assert_eq!(Some(card.cases as u64), expected["cases"].as_u64());
    for (what, actual) in [
        ("recall_at_1", card.recall_at_1),
        ("recall_at_5", card.recall_at_5),
        ("recall_at_10", card.recall_at_10),
        ("mean_reciprocal_rank", card.mean_reciprocal_rank),
        ("ndcg_at_10", card.ndcg_at_10),
        ("answer_core_precision", card.answer_core_precision),
        ("false_unknown_rate", card.false_unknown_rate),
        ("mean_used_bytes", card.mean_used_bytes),
        ("mean_elapsed_millis", card.mean_elapsed_millis),
    ] {
        assert_close("scorecard", what, actual, &expected[what], tolerance);
    }
}

#[test]
fn every_returned_ref_normalizes_to_what_the_shared_fixture_says() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture is JSON");
    let refs = fixture["refs"].as_array().expect("refs");
    assert!(!refs.is_empty());
    for case in refs {
        let value = case["value"].as_str().expect("returned ref");
        let expected = case["normalized"].as_str().expect("normalized ref");
        assert_eq!(normalize(value), expected, "normalize({value:?})");
    }
}

#[test]
fn every_evidence_list_reads_as_the_shared_fixture_says() {
    let fixture: Value = serde_json::from_str(FIXTURE).expect("fixture is JSON");
    let lists = fixture["retrieved"].as_array().expect("retrieved");
    assert!(!lists.is_empty());
    for case in lists {
        let ids = strings(&case["ids"]);
        let read = retrieved(ids.iter().map(String::as_str));
        assert_eq!(read, strings(&case["retrieved"]), "retrieved({ids:?})");
    }
}
