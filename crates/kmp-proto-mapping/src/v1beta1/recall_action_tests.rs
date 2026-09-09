//! Core restoration is an action even after the last expansion item.
use super::*;

#[test]
fn a_completed_expansion_restores_its_shortened_cited_core() {
    let source = "Exact canonical evidence, including amounts 91.25 and 18.50. ".repeat(400);
    let packet = json!({"summary":"Selected evidence", "answer":"Source establishes the claim",
        "because":[{"ref":"evidence:source","claim":"claim:one","evidence":""}],
        "proof":{"path":[],"evidence":[{"id":"evidence:source","text":source,
            "source":"source:one","supports":["claim:one"]}],"missing":[]},"warnings":[]});
    let arguments = json!({"about":"project:core","question":"What amounts?",
        "budget":{"detail":"full","max_bytes":512}});
    let projected = |args: &Value| match project_recall_output_typed(
        packet.clone(),
        args,
        2400,
        Cl100kEstimator::shared(),
    )
    .expect("projection")
    {
        ProjectionOutcome::Projected(value) => value,
        ProjectionOutcome::CoreTooLarge => panic!("floor must return a recovery call"),
    };
    let floor = projected(&arguments);
    assert_eq!(floor["projection"]["page"]["has_more"], false);
    assert_eq!(floor["projection"]["core_text_shortened"], true);
    let call = &floor["projection"]["next_action"];
    assert_eq!(call["tool"], "kmp_ask");
    assert!(call["arguments"]["page"].get("cursor").is_none());
    let restored = projected(&call["arguments"]);
    assert_eq!(restored["projection"]["core_text_shortened"], false);
    assert!(restored["projection"]["next_action"].is_null());
    assert_eq!(restored["proof"]["evidence"], packet["proof"]["evidence"]);
    assert_eq!(
        restored["projection"]["budget"]["used_bytes"],
        serialized_bytes(&restored)
    );
}

#[test]
fn typed_call_keeps_integers_above_floating_point_precision() {
    let call = json!({"tool":"kmp_wake","arguments":{"about":"project:integer",
        "budget":{"max_bytes":9_007_199_254_740_993_u64}}});
    let wire = actions::call_from_value(&call);
    assert_eq!(actions::call_value(&wire), call);
}
