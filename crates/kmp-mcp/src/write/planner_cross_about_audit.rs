//! Writer equivalence declarations across abouts require a scoped proposal.
#![cfg(test)]

use super::planner::build_write_plan_with_root;
use super::planner_audit::tests::sample_write_request;
use serde_json::{Value, json};

fn cross_about_request() -> Value {
    let mut request = sample_write_request();
    // A declaration of identity carries no delta of its own: the delta
    // relation would point at the other about too, and only the
    // equivalence may.
    request
        .as_object_mut()
        .expect("request object")
        .remove("semantic_delta");
    request["connect_to"] = json!([{
        "ref": "incident:platform:outcome:freeze",
        "rel": "same_event_as",
        "class": "evidential",
        "why": "Both record the same freeze.",
        "evidence": "kmp_relate proposal by identifier.",
        "confidence": "high"
    }]);
    request["read_context"] = json!({
        "relate_proposals": [{
            "from": "incident:mobile-login:observation:401-refresh-race",
            "to": "incident:platform:outcome:freeze",
            "proposed_by": ["identifier", "entity"]
        }]
    });
    request["options"] = json!({"dry_run": true, "strict": true});
    request
}

/// The one relation that crosses an about: declared from a proposal,
/// stamped with it, its evidence claiming only what this about owns.
#[test]
fn an_equivalence_to_another_about_is_declared_from_its_proposal() {
    let plan = build_write_plan_with_root(&cross_about_request(), false)
        .expect("a declared equivalence is accepted");
    let relation = &plan.ingest_arguments["memory"]["relations"][0];
    assert_eq!(relation["rel"], "same_event_as");
    assert_eq!(relation["to"], "incident:platform:outcome:freeze");
    assert_eq!(relation["method"], "kmp_relate:identifier+entity");
    let evidence = &plan.ingest_arguments["memory"]["evidence"][0];
    assert_eq!(
        evidence["supports"].as_array().map(Vec::len),
        Some(1),
        "the evidence node claims this about's entry, not the other about's: {evidence}"
    );
    assert_eq!(plan.relation_quality[0]["crosses_about"], true);
    assert_eq!(
        plan.relation_quality[0]["prior_context_sources"],
        json!(["kmp_relate"])
    );
}

#[test]
fn any_other_relation_across_abouts_and_an_unproven_equivalence_are_refused() {
    let mut follows = cross_about_request();
    follows["connect_to"][0]["rel"] = json!("follows");
    follows["connect_to"][0]["class"] = json!("procedural");
    let error = build_write_plan_with_root(&follows, false).expect_err("no other relation crosses");
    assert!(
        error
            .message
            .contains("only with `same_event_as` or `same_entity_as`"),
        "{error}"
    );

    let mut unproven = cross_about_request();
    unproven["read_context"] = json!({"inspected_refs": ["incident:platform:outcome:freeze"]});
    let error =
        build_write_plan_with_root(&unproven, false).expect_err("an inspection is not a proposal");
    assert!(
        error.message.contains("read_context.relate_proposals"),
        "{error}"
    );

    let mut foreign_pair = cross_about_request();
    foreign_pair["read_context"]["relate_proposals"][0]["from"] = json!("incident:other:entry:x");
    let error = build_write_plan_with_root(&foreign_pair, false)
        .expect_err("one ref of the proposal must be this about's");
    assert!(
        error.message.contains("read_context.relate_proposals"),
        "{error}"
    );
}
