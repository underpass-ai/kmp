use super::*;

fn response(state: &str) -> Value {
    json!({
        "proof": {
            "manifest_id": "m1",
            "delivery": {"admitted_record_bytes": 148, "deferred_budget": 1}
        },
        "objects": [
            {"ref": "a", "body_state": "loaded"},
            {"ref": "source", "body_state": state, "required_record_bytes": 113}
        ]
    })
}

fn arguments() -> Value {
    json!({
        "about": "project:test",
        "from": "a",
        "to": "b",
        "search": {"proof": true, "max_body_record_bytes": 148,
                   "proof_refs": ["a"], "expect_selection": "older"},
        "page": {"cursor": "stale"},
        "budget": {"max_bytes": 8000}
    })
}

#[test]
fn the_next_batch_names_what_is_pending_and_binds_it_to_this_manifest() {
    let mut value = response("deferred_budget");

    attach(&mut value, &arguments());

    let action = &value["proof"]["expand_bodies"];
    assert_eq!(action["tool"], "kmp_trace");
    let search = &action["arguments"]["search"];
    assert_eq!(search["proof_refs"], json!(["source"]));
    assert_eq!(search["expect_selection"], "m1");
    assert_eq!(
        search["max_body_record_bytes"], 113,
        "the allowance is exactly what the named record needs, not the old prefix"
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
fn a_carded_body_is_pending_too_because_its_canonical_text_was_not_delivered() {
    let mut value = response("compact");

    attach(&mut value, &arguments());

    assert_eq!(
        value["proof"]["expand_bodies"]["arguments"]["search"]["proof_refs"],
        json!(["source"])
    );
}

#[test]
fn a_complete_delivery_offers_no_expansion() {
    let mut value = response("loaded");

    attach(&mut value, &arguments());

    assert_eq!(value["proof"]["expand_bodies"], Value::Null);
}

#[test]
fn a_refusal_offers_a_fresh_read_without_the_expectation_it_just_refused() {
    let mut value = json!({
        "expansion_refusal": {"code": "read_selection_changed", "expected": "older", "actual": "m2"},
        "objects": []
    });

    attach(&mut value, &arguments());

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


#[test]
fn a_batch_under_a_declared_ceiling_names_the_prefix_that_fits() {
    let mut value = json!({
        "proof": {"manifest_id": "m1", "delivery": {"admitted_record_bytes": 0}},
        "objects": [
            {"ref": "a", "body_state": "not_requested", "required_record_bytes": 60},
            {"ref": "b", "body_state": "not_requested", "required_record_bytes": 60},
            {"ref": "c", "body_state": "not_requested", "required_record_bytes": 60}
        ]
    });
    let mut arguments = arguments();
    arguments["search"]["max_body_record_bytes"] = json!(130);

    attach(&mut value, &arguments);

    let search = &value["proof"]["expand_bodies"]["arguments"]["search"];
    assert_eq!(search["proof_refs"], json!(["a", "b"]));
    assert_eq!(search["max_body_record_bytes"], 120);
}

#[test]
fn a_record_larger_than_the_declared_ceiling_is_offered_with_its_exact_price() {
    let mut value = json!({
        "proof": {"manifest_id": "m1", "delivery": {"admitted_record_bytes": 0}},
        "objects": [{"ref": "huge", "body_state": "deferred_budget",
                     "required_record_bytes": 67_108_864}]
    });
    let mut arguments = arguments();
    arguments["search"]["max_body_record_bytes"] = json!(8_388_608);

    attach(&mut value, &arguments);

    let search = &value["proof"]["expand_bodies"]["arguments"]["search"];
    assert_eq!(search["proof_refs"], json!(["huge"]));
    assert_eq!(
        search["max_body_record_bytes"], 67_108_864,
        "the price is named exactly; running the action stays the caller's choice"
    );
}
