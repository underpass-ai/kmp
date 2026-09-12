use super::*;

fn response(plan: Value) -> Value {
    json!({"proof": {"manifest_id": "m1", "expansion_plan": plan}})
}

fn arguments() -> Value {
    json!({
        "about": "project:test",
        "from": "a",
        "to": "b",
        "search": {"proof": true, "max_body_record_bytes": 148},
        "page": {"cursor": "stale", "entries": 128},
        "budget": {"max_bytes": 8000}
    })
}

fn search_of(action: &Value) -> &Value {
    &action["arguments"]["search"]
}

#[test]
fn the_plan_becomes_a_complete_call_with_the_query_and_the_manifest() {
    let mut value = response(json!({"refs": ["b", "c"], "record_bytes": 148}));

    attach(&mut value, &arguments());

    let action = &value["proof"]["expand_bodies"];
    assert_eq!(action["tool"], "kmp_trace");
    let search = search_of(action);
    assert_eq!(search["proof_refs"], json!(["b", "c"]));
    assert_eq!(search["expect_selection"], "m1");
    assert_eq!(search["max_body_record_bytes"], 148);
    assert_eq!(action["arguments"]["about"], "project:test");
}

#[test]
fn an_expansion_keeps_the_page_size_and_drops_only_the_cursor() {
    // Dropping the whole page re-partitioned every expansion into the default
    // size, which is how actions and pending refs stopped lining up.
    let mut value = response(json!({"refs": ["b"], "record_bytes": 148}));

    attach(&mut value, &arguments());

    let page = &value["proof"]["expand_bodies"]["arguments"]["page"];
    assert_eq!(page["entries"], 128);
    assert_eq!(page["cursor"], Value::Null);
}

#[test]
fn an_oversized_record_is_offered_separately_at_its_exact_price() {
    let mut value = response(json!({
        "refs": ["small"], "record_bytes": 148,
        "oversized_ref": "huge", "oversized_record_bytes": 67_108_864
    }));

    attach(&mut value, &arguments());

    assert_eq!(
        search_of(&value["proof"]["expand_bodies"])["proof_refs"],
        json!(["small"]),
        "the rest of the suffix stays recoverable under the chosen ceiling"
    );
    let oversized = search_of(&value["proof"]["expand_oversized_body"]);
    assert_eq!(oversized["proof_refs"], json!(["huge"]));
    assert_eq!(oversized["max_body_record_bytes"], 67_108_864);
}

#[test]
fn only_the_oversized_record_left_still_produces_its_own_call() {
    let mut value = response(json!({
        "refs": [], "record_bytes": 148,
        "oversized_ref": "huge", "oversized_record_bytes": 900
    }));

    attach(&mut value, &arguments());

    assert_eq!(value["proof"]["expand_bodies"], Value::Null);
    assert_eq!(
        search_of(&value["proof"]["expand_oversized_body"])["proof_refs"],
        json!(["huge"])
    );
}

#[test]
fn a_response_the_kernel_left_without_a_plan_offers_nothing() {
    let mut value = json!({"proof": {"manifest_id": "m1"}});

    attach(&mut value, &arguments());

    assert_eq!(value["proof"]["expand_bodies"], Value::Null);
    assert_eq!(value["proof"]["expand_oversized_body"], Value::Null);
}

#[test]
fn a_refusal_offers_a_fresh_read_without_the_expectation_it_just_refused() {
    let mut value = json!({
        "expansion_refusal": {"code": "read_selection_changed", "expected": "older", "actual": "m2"}
    });
    let mut arguments = arguments();
    arguments["search"]["proof_refs"] = json!(["source"]);
    arguments["search"]["expect_selection"] = json!("older");

    attach(&mut value, &arguments);

    let fresh = &value["expansion_refusal"]["fresh_read"];
    assert_eq!(fresh["tool"], "kmp_trace");
    assert_eq!(fresh["arguments"]["search"]["proof_refs"], Value::Null);
    assert_eq!(
        fresh["arguments"]["search"]["expect_selection"], Value::Null,
        "repeating the refused expectation would repeat the refusal"
    );
    assert_eq!(fresh["arguments"]["search"]["proof"], true);
    assert_eq!(fresh["arguments"]["page"]["cursor"], Value::Null);
}
