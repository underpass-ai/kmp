//! The relabel planner, audited: what the arguments alone can show is
//! refused here with the field named; everything the store must be read
//! for is left to the kernel and reaches the backend as the kernel's own
//! request shape.
#![cfg(test)]

use serde_json::{Value, json};

use super::relabel_planner::build_relabel_plan;

fn arguments() -> Value {
    json!({
        "about": "project:kmp",
        "ref": "project:kmp:decision:relabel",
        "actor": "claude",
        "observed_at": "2026-09-05T12:00:00Z",
        "why": "The decision belongs to the issue it closed.",
        "add": {"issue": "506"},
        "remove": {"task": "launch"}
    })
}

#[test]
fn the_plan_compiles_pairs_to_the_kernel_request() {
    let plan = build_relabel_plan(&arguments()).expect("a valid relabel plans");

    assert_eq!(plan.about, "project:kmp");
    assert_eq!(plan.reference, "project:kmp:decision:relabel");
    assert!(!plan.dry_run);
    assert_eq!(plan.add, [json!({"key": "issue", "value": "506"})]);
    assert_eq!(plan.remove, [json!({"key": "task", "value": "launch"})]);
    let request = &plan.relabel_arguments;
    assert_eq!(request["about"], "project:kmp");
    assert_eq!(request["ref"], "project:kmp:decision:relabel");
    assert_eq!(request["add"][0]["key"], "issue");
    assert_eq!(request["remove"][0]["value"], "launch");
    assert_eq!(
        request["why"],
        "The decision belongs to the issue it closed."
    );
    assert_eq!(request["label_policy"], "refuse", "strict by default");
    assert_eq!(request["dry_run"], false);
    assert_eq!(request["provenance"]["source_kind"], "agent");
    assert_eq!(request["provenance"]["source_agent"], "claude");
    assert_eq!(request["provenance"]["observed_at"], "2026-09-05T12:00:00Z");
    assert!(
        request["idempotency_key"]
            .as_str()
            .is_some_and(|key| key.starts_with("relabel:")),
        "{request}"
    );
    assert_eq!(request["intended_new"], json!([]));
}

#[test]
fn the_idempotency_key_ignores_whether_the_call_previews() {
    let committed = build_relabel_plan(&arguments()).expect("plans");
    let mut previewing = arguments();
    previewing["options"] = json!({"dry_run": true});
    let previewed = build_relabel_plan(&previewing).expect("plans");
    assert_eq!(
        committed.relabel_arguments["idempotency_key"],
        previewed.relabel_arguments["idempotency_key"]
    );
    assert!(previewed.dry_run);
    assert_eq!(previewed.relabel_arguments["dry_run"], true);
}

#[test]
fn options_reach_the_kernel_as_its_policy_and_insistence() {
    let mut arguments = arguments();
    arguments["options"] = json!({"strict": false, "labels_new": ["issue"]});
    let plan = build_relabel_plan(&arguments).expect("plans");
    assert_eq!(plan.relabel_arguments["label_policy"], "warn");
    assert_eq!(plan.relabel_arguments["intended_new"], json!(["issue"]));

    arguments["options"] = json!({"labels_new": ["component"]});
    let error = build_relabel_plan(&arguments).expect_err("insisting on a key not added");
    assert!(
        error.contains("`options.labels_new` names `component`"),
        "{error}"
    );
}

#[test]
fn nothing_to_relabel_and_a_pair_on_both_sides_are_refused() {
    let mut arguments = arguments();
    arguments.as_object_mut().expect("object").remove("add");
    arguments.as_object_mut().expect("object").remove("remove");
    let error = build_relabel_plan(&arguments).expect_err("nothing to do");
    assert!(error.contains("nothing to relabel"), "{error}");

    let mut arguments = self::arguments();
    arguments["remove"] = json!({"issue": "506"});
    let error = build_relabel_plan(&arguments).expect_err("both sides");
    assert!(
        error.contains("`issue=506` is both added and removed"),
        "{error}"
    );
}

#[test]
fn a_key_a_filter_cannot_name_and_a_value_used_twice_are_refused_at_their_field() {
    let mut arguments = arguments();
    arguments["add"] = json!({"Issue": "506"});
    let error = build_relabel_plan(&arguments).expect_err("a bad key");
    assert!(error.contains("`add.Issue` is not a label key"), "{error}");

    arguments["add"] = json!({"issue": "506", "ticket": "506"});
    let error = build_relabel_plan(&arguments).expect_err("one value, one key");
    assert!(error.contains("`add` uses `506` under two keys"), "{error}");

    arguments["add"] = json!({"issue": ""});
    let error = build_relabel_plan(&arguments).expect_err("an empty value");
    assert!(
        error.contains("`add.issue` must be a non-empty scope id"),
        "{error}"
    );
}

#[test]
fn the_ref_must_be_an_entry_of_this_about() {
    let mut arguments = arguments();
    arguments["ref"] = json!("project:other:decision:relabel");
    let error = build_relabel_plan(&arguments).expect_err("another about");
    assert!(
        error.contains("does not belong to about `project:kmp`"),
        "{error}"
    );

    arguments["ref"] = json!("project:kmp");
    let error = build_relabel_plan(&arguments).expect_err("the anchor");
    assert!(error.contains("cannot replace the about anchor"), "{error}");
}

#[test]
fn a_time_that_has_not_happened_is_refused_like_a_write() {
    let mut arguments = arguments();
    arguments["observed_at"] = json!("2099-01-01T00:00:00Z");
    let error = build_relabel_plan(&arguments).expect_err("the future");
    assert!(error.contains("has not happened yet"), "{error}");
}
