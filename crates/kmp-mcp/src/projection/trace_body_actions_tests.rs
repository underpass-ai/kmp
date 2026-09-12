use super::*;

fn object(reference: &str, state: &str, cost: u64) -> Value {
    json!({"ref": reference, "body_state": state, "required_record_bytes": cost})
}

fn response(objects: Vec<Value>) -> Value {
    json!({
        "proof": {"manifest_id": "m1", "delivery": {"admitted_record_bytes": 0}},
        "objects": objects
    })
}

fn arguments() -> Value {
    json!({
        "about": "project:test",
        "from": "a",
        "to": "b",
        "search": {"proof": true},
        "page": {"cursor": "stale"},
        "budget": {"max_bytes": 8000}
    })
}

fn with_ceiling(ceiling: u64) -> Value {
    let mut arguments = arguments();
    arguments["search"]["max_body_record_bytes"] = json!(ceiling);
    arguments
}

fn named(ceiling: u64, refs: &[&str]) -> Value {
    let mut arguments = with_ceiling(ceiling);
    arguments["search"]["proof_refs"] = json!(refs);
    arguments["search"]["expect_selection"] = json!("m1");
    arguments
}

fn batch(value: &Value) -> Vec<String> {
    value["proof"]["expand_bodies"]["arguments"]["search"]["proof_refs"]
        .as_array()
        .map(|refs| {
            refs.iter()
                .map(|reference| reference.as_str().expect("ref").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn allowance(value: &Value) -> Option<u64> {
    value["proof"]["expand_bodies"]["arguments"]["search"]["max_body_record_bytes"].as_u64()
}

#[test]
fn the_next_batch_names_what_is_pending_and_binds_it_to_this_manifest() {
    let mut value = response(vec![
        object("a", "loaded", 60),
        object("source", "deferred_budget", 113),
    ]);

    attach(&mut value, &with_ceiling(148));

    let action = &value["proof"]["expand_bodies"];
    assert_eq!(action["tool"], "kmp_trace");
    assert_eq!(batch(&value), vec!["source"]);
    let search = &action["arguments"]["search"];
    assert_eq!(search["expect_selection"], "m1");
    assert_eq!(
        search["max_body_record_bytes"], 148,
        "the ceiling the caller chose is carried through, not replaced by the batch total"
    );
    assert_eq!(
        action["arguments"]["about"], "project:test",
        "the bound query is carried, not rebuilt by the caller"
    );
    assert_eq!(
        action["arguments"]["page"],
        Value::Null,
        "a page position belongs to one projection, never to an expansion"
    );
}

#[test]
fn a_batch_never_exceeds_the_public_expansion_limit() {
    // A descriptor-only read of a large selection with room to spare. Naming
    // every pending ref would produce a call our own parser refuses, which is
    // not a copyable action.
    let objects: Vec<Value> = (0..MAX_EXPANSION_REFS + 40)
        .map(|index| object(&format!("ref-{index:03}"), "not_requested", 10))
        .collect();
    let mut value = response(objects);

    attach(&mut value, &with_ceiling(1_000_000));

    let refs = batch(&value);
    assert_eq!(refs.len(), MAX_EXPANSION_REFS);
    assert_eq!(refs[0], "ref-000", "and it starts at the head of the suffix");
}

#[test]
fn successive_actions_walk_the_selection_to_its_end_without_repeating() {
    // One record fits at a time. Following the offered action each round must
    // reach the end: no ref twice, no return to the prefix, no growing ceiling.
    let names: Vec<String> = (0..6).map(|index| format!("ref-{index}")).collect();
    let ceiling = 60;
    let mut recovered: Vec<String> = Vec::new();
    let mut arguments = with_ceiling(ceiling);

    for round in 0..12 {
        let objects: Vec<Value> = names
            .iter()
            .map(|reference| {
                let state = if recovered.contains(reference) {
                    "loaded"
                } else {
                    "not_requested"
                };
                object(reference, state, 60)
            })
            .collect();
        let mut value = response(objects);
        attach(&mut value, &arguments);

        let refs = batch(&value);
        if refs.is_empty() {
            assert_eq!(
                recovered.len(),
                names.len(),
                "the chain stopped before the end on round {round}"
            );
            break;
        }
        assert_eq!(
            allowance(&value),
            Some(ceiling),
            "the ceiling never grows to make progress"
        );
        for reference in &refs {
            assert!(
                !recovered.contains(reference),
                "round {round} asked again for `{reference}`"
            );
            recovered.push(reference.clone());
        }
        arguments = named(ceiling, &refs.iter().map(String::as_str).collect::<Vec<_>>());
    }

    assert_eq!(recovered, names, "every body was recovered exactly once");
}

#[test]
fn a_requested_body_that_did_not_fit_is_offered_again_rather_than_walked_past() {
    // The caller asked for `b` and got nothing: the frontier must not advance
    // past a ref that was never loaded.
    let mut value = response(vec![
        object("a", "loaded", 60),
        object("b", "deferred_budget", 60),
        object("c", "not_requested", 60),
    ]);

    attach(&mut value, &named(60, &["b"]));

    assert_eq!(batch(&value), vec!["b"]);
}

#[test]
fn a_record_over_the_ceiling_is_kept_pending_and_offered_on_its_own() {
    let mut value = response(vec![
        object("huge", "deferred_budget", 67_108_864),
        object("small", "not_requested", 60),
    ]);

    attach(&mut value, &with_ceiling(8_388_608));

    assert_eq!(
        batch(&value),
        vec!["small"],
        "the rest of the suffix stays recoverable under the ceiling the caller chose"
    );
    assert_eq!(allowance(&value), Some(8_388_608));
    let oversized = &value["proof"]["expand_oversized_body"]["arguments"]["search"];
    assert_eq!(oversized["proof_refs"], json!(["huge"]));
    assert_eq!(
        oversized["max_body_record_bytes"], 67_108_864,
        "its exact price, in a separate call the caller has to choose to make"
    );
}

#[test]
fn without_a_ceiling_the_batch_carries_its_own_exact_total() {
    let mut value = response(vec![
        object("a", "not_requested", 40),
        object("b", "not_requested", 60),
    ]);

    attach(&mut value, &arguments());

    assert_eq!(batch(&value), vec!["a", "b"]);
    assert_eq!(allowance(&value), Some(100));
}

#[test]
fn a_carded_body_is_pending_too_because_its_canonical_text_was_not_delivered() {
    let mut value = response(vec![object("source", "compact", 113)]);

    attach(&mut value, &with_ceiling(200));

    assert_eq!(batch(&value), vec!["source"]);
}

#[test]
fn a_complete_delivery_offers_no_expansion() {
    let mut value = response(vec![object("a", "loaded", 60), object("b", "loaded", 60)]);

    attach(&mut value, &with_ceiling(200));

    assert_eq!(value["proof"]["expand_bodies"], Value::Null);
    assert_eq!(value["proof"]["expand_oversized_body"], Value::Null);
}

#[test]
fn a_body_the_store_lacks_never_blocks_the_suffix() {
    let mut value = response(vec![
        object("a", "loaded", 60),
        object("gone", "missing", 0),
        object("c", "not_requested", 60),
    ]);

    attach(&mut value, &with_ceiling(60));

    assert_eq!(batch(&value), vec!["c"]);
}

#[test]
fn a_refusal_offers_a_fresh_read_without_the_expectation_it_just_refused() {
    let mut value = json!({
        "expansion_refusal": {"code": "read_selection_changed", "expected": "older", "actual": "m2"},
        "objects": []
    });

    attach(&mut value, &named(148, &["source"]));

    let fresh = &value["expansion_refusal"]["fresh_read"];
    assert_eq!(fresh["tool"], "kmp_trace");
    let search = &fresh["arguments"]["search"];
    assert_eq!(search["proof_refs"], Value::Null);
    assert_eq!(
        search["expect_selection"], Value::Null,
        "repeating the refused expectation would repeat the refusal"
    );
    assert_eq!(search["proof"], true, "the selection itself is unchanged");
    assert_eq!(fresh["arguments"]["page"], Value::Null);
}
