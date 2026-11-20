//! The write switches, audited: `dry_run` and `strict` are JSON booleans or
//! absent. Anything else is refused at its own field by every planner that
//! reads them, never read as the default. QA found `dry_run: "true"`
//! committing a memory the caller meant only to preview (13 nov 2026).
#![cfg(test)]

use serde_json::{Map, Value, json};

use super::batch_planner::build_batch_plan;
use super::planner::build_write_plan_with_root;
use super::planner_audit::tests::sample_write_request;
use super::relabel_planner::build_relabel_plan;
use super::relation_planner::build_relation_plan;
use super::write_options::WriteOptions;
use crate::serving::ToolError;

/// Every value a caller might mean as a switch that is not a JSON boolean.
fn mistyped() -> [Value; 7] {
    [
        json!("true"),
        json!("false"),
        json!(1),
        json!(0),
        Value::Null,
        json!([true]),
        json!({"value": true}),
    ]
}

fn with_options(mut arguments: Value, options: Value) -> Value {
    arguments["options"] = options;
    arguments
}

fn packet() -> Value {
    json!({
        "about": "project:switches", "actor": "auditor",
        "observed_at": "2026-11-13T03:30:38Z",
        "labels": {"topic": ["qa"]},
        "memories": [{"id": "m", "kind": "observation", "summary": "QA test",
            "evidence": "sandbox probe"}]
    })
}

fn relations() -> Value {
    json!({
        "about": "project:switches", "actor": "auditor",
        "observed_at": "2026-11-13T03:30:38Z",
        "relations": [{
            "from": "project:switches:observation:a", "to": "project:switches:decision:b",
            "rel": "supports", "class": "evidential", "confidence": "high",
            "why": "The observation is the reason for the decision.",
            "evidence": "Report R1."
        }]
    })
}

fn relabel() -> Value {
    json!({
        "about": "project:switches", "ref": "project:switches:decision:b",
        "actor": "auditor", "observed_at": "2026-11-13T03:30:38Z",
        "why": "The decision belongs to the QA topic.",
        "add": {"topic": ["qa"]}
    })
}

fn feedback(error: impl Into<ToolError>) -> Vec<Value> {
    error.into().feedback
}

fn object(value: &Value) -> &Map<String, Value> {
    value.as_object().expect("arguments object")
}

#[test]
fn absent_switches_take_their_defaults() {
    let options = WriteOptions::from_arguments(object(&packet())).expect("no options");
    assert_eq!(options, WriteOptions::default());
    assert!(
        !options.dry_run,
        "a write commits unless the preview is asked"
    );
    assert!(options.strict);

    let empty = with_options(packet(), json!({}));
    assert_eq!(
        WriteOptions::from_arguments(object(&empty)).expect("empty options"),
        WriteOptions::default()
    );
}

#[test]
fn a_boolean_switch_is_read_as_given() {
    let preview = with_options(packet(), json!({"dry_run": true, "strict": false}));
    let options = WriteOptions::from_arguments(object(&preview)).expect("booleans");
    assert!(options.dry_run);
    assert!(!options.strict);
}

#[test]
fn a_switch_of_any_other_type_is_refused_at_its_field() {
    for key in ["dry_run", "strict"] {
        for value in mistyped() {
            let arguments = with_options(packet(), json!({ key: value.clone() }));
            let error = WriteOptions::from_arguments(object(&arguments))
                .expect_err("a switch is never coerced");
            let feedback = feedback(error);
            assert_eq!(feedback.len(), 1, "{key}={value}: {feedback:?}");
            assert_eq!(feedback[0]["field"], format!("options.{key}"));
            assert_eq!(feedback[0]["code"], "INVALID_TYPE");
            assert_eq!(feedback[0]["expected_type"], "boolean");
        }
    }
}

#[test]
fn options_that_are_not_an_object_are_refused() {
    for options in [json!("{\"dry_run\":true}"), json!(true), json!([])] {
        let arguments = with_options(packet(), options.clone());
        let feedback = feedback(
            WriteOptions::from_arguments(object(&arguments)).expect_err("options is an object"),
        );
        assert_eq!(feedback[0]["field"], "options", "{options}");
        assert_eq!(feedback[0]["expected_type"], "object");
    }
}

/// The QA replay: a packet of records with `dry_run: "true"` planned as a
/// commit. It is refused once, at `options.dry_run`, not once per record.
#[test]
fn a_packet_with_a_mistyped_dry_run_is_refused_once() {
    let mut arguments = with_options(packet(), json!({"dry_run": "true"}));
    arguments["memories"]
        .as_array_mut()
        .expect("memories")
        .push(
            json!({"id": "n", "kind": "observation", "summary": "QA test two",
            "evidence": "sandbox probe"}),
        );
    let feedback = feedback(build_batch_plan(&arguments).expect_err("never a commit"));
    let fields = feedback
        .iter()
        .map(|item| item["field"].as_str().expect("field"))
        .collect::<Vec<_>>();
    assert_eq!(fields, ["options.dry_run"], "{feedback:?}");

    let preview = build_batch_plan(&with_options(packet(), json!({"dry_run": true})))
        .expect("a boolean preview plans");
    assert!(preview.dry_run);
    assert_eq!(preview.ingest_arguments["dry_run"], true);
}

#[test]
fn every_planner_refuses_a_mistyped_switch() {
    for key in ["dry_run", "strict"] {
        let options = json!({ key: "true" });
        let field = format!("options.{key}");

        let batch = feedback(
            build_batch_plan(&with_options(packet(), options.clone()))
                .expect_err("memories refuse it"),
        );
        assert_eq!(batch[0]["field"], field, "memories {key}");

        let links = feedback(
            build_relation_plan(&with_options(relations(), options.clone()))
                .expect_err("relations refuse it"),
        );
        assert_eq!(links[0]["field"], field, "relations {key}");

        let relabel = build_relabel_plan(&with_options(relabel(), options.clone()))
            .expect_err("relabel refuses it");
        assert!(relabel.contains(&field), "relabel {key}: {relabel}");

        // The single-record planner, reached by older callers and the batch.
        let single = build_write_plan_with_root(
            &with_options(sample_write_request(), options.clone()),
            false,
        )
        .expect_err("the single planner refuses it");
        assert!(single.field_is_within(&field), "single {key}: {single:?}");
    }
}

#[test]
fn every_planner_still_previews_on_a_boolean() {
    let preview = json!({"dry_run": true});
    assert!(
        build_relation_plan(&with_options(relations(), preview.clone()))
            .expect("relations preview")
            .dry_run
    );
    assert!(
        build_relabel_plan(&with_options(relabel(), preview))
            .expect("relabel preview")
            .dry_run
    );
}
