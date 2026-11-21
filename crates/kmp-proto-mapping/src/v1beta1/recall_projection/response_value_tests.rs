//! What the rendered wake JSON carries, and what reading it back restores.

use super::json_paths::array_len;
use super::response_value::{ask_value, wake_value};
use super::test_support::{labels_fixture, typed_ask_fixture, typed_wake_fixture};
use super::typed_response::{apply_ask_value, apply_wake_value};

#[test]
fn wake_value_carries_the_labels_and_apply_reads_them_back() {
    let mut response = typed_wake_fixture(2);
    response.labels = labels_fixture(3);

    let value = wake_value(&response);

    assert_eq!(array_len(&value, &["labels"]), 3);
    assert_eq!(value["labels"][0]["about"], "project:kmp");
    assert_eq!(value["labels"][0]["key"], "task");
    assert_eq!(value["labels"][0]["value"], "underpass-ai-kmp-000");
    assert_eq!(value["labels"][0]["entries"], 3);
    assert!(value["labels"][0]["last_observed_at"].is_string());
    let applied = apply_wake_value(typed_wake_fixture(2), &value);
    assert_eq!(applied.labels, response.labels);
}

/// An ask's path is rendered as the mapping joined it (to the head's
/// evidence, P14): a tail item that repeats a hop's body, carried only by a
/// deeper reading, must not change the hop, or every continuation cursor
/// bound to the hop is refused (`READ_SELECTION_CHANGED`).
#[test]
fn an_ask_path_does_not_change_with_the_tail_a_reading_carries() {
    let shallow = typed_ask_fixture(3);
    let mut deep = shallow.clone();
    let proof = deep.proof.as_mut().expect("proof");
    let hop = proof.path[0].clone();
    proof.path[0].evidence = "The order names the pump.".to_string();
    // The deeper reading carries, in its tail, the hop's relation evidence.
    let mut tail = proof.evidence[0].clone();
    tail.id = format!("detail:evidence:{}:relation:1", hop.source_ref);
    tail.supports = vec![hop.source_ref.clone()];
    tail.text = "The order names the pump.".to_string();
    proof.evidence.push(tail);
    let mut shallow = shallow;
    shallow.proof.as_mut().expect("proof").path[0].evidence =
        "The order names the pump.".to_string();

    let shallow_value = ask_value(&shallow);
    let deep_value = ask_value(&deep);

    assert_eq!(shallow_value["proof"]["path"], deep_value["proof"]["path"]);
    assert_eq!(
        deep_value["proof"]["path"][0]["evidence"],
        "The order names the pump."
    );
    assert!(
        deep_value["proof"]["path"][0]
            .get("evidence_refs")
            .is_none()
    );
    // Read back, the typed path is the one rendered.
    let applied = apply_ask_value(deep.clone(), &deep_value);
    assert_eq!(
        applied.proof.expect("proof").path,
        deep.proof.expect("proof").path
    );
}

/// A hop a page places after the evidence (`supports`) is paged only by a
/// reading that carries every evidence item, so it is joined to all of
/// them, as it always was.
#[test]
fn an_ask_hop_paged_after_the_evidence_is_joined_to_all_of_it() {
    let mut response = typed_ask_fixture(3);
    let proof = response.proof.as_mut().expect("proof");
    assert_eq!(proof.path[1].rel, "supports");
    let hop = proof.path[1].clone();
    proof.path[1].evidence = "The sheet shows a diluted batch.".to_string();
    let mut tail = proof.evidence[0].clone();
    tail.id = format!("detail:evidence:{}:relation:1", hop.source_ref);
    tail.supports = vec![hop.source_ref.clone()];
    tail.text = "The sheet shows a diluted batch.".to_string();
    proof.evidence.push(tail.clone());

    let value = ask_value(&response);

    let rendered = &value["proof"]["path"][1];
    assert!(rendered.get("evidence").is_none(), "{rendered}");
    assert!(
        rendered["evidence_refs"]
            .as_array()
            .expect("refs")
            .iter()
            .any(|id| id == tail.id.as_str()),
        "{rendered}"
    );
    let applied = apply_ask_value(response, &value);
    assert!(
        applied.proof.expect("proof").path[1]
            .evidence_refs
            .contains(&tail.id)
    );
}
