//! The batch compiler, audited for one pass: every failure of every record
//! comes back together, a failure found while reading the packet stands in
//! for the compiler's derived one, and what makes a packet unreadable still
//! stops it at once.
#![cfg(test)]

use serde_json::{Value, json};

use super::batch_planner::build_batch_plan;
use crate::serving::ToolError;

fn packet(memories: Value) -> Value {
    json!({"about":"project:one-pass","actor":"auditor","observed_at":"2026-09-09T10:00:00Z",
        "memories": memories})
}

fn refused_fields(arguments: &Value) -> Vec<String> {
    let error = ToolError::from(build_batch_plan(arguments).expect_err("refused"));
    error
        .feedback
        .iter()
        .map(|item| item["field"].as_str().expect("field").to_owned())
        .collect()
}

#[test]
fn every_rule_of_a_record_is_reported_in_one_pass() {
    let arguments = packet(json!([{
        "id":"a","kind":"outcome","summary":"La ruta abre el martes según el aviso #42.",
        "observed_at":"2999-01-01T00:00:00Z",
        "connect_to":[
            {"ref":"project:one-pass:entry:x","rel":"supports"},
            {"ref":"project:one-pass:entry:y","rel":"no_such_relation"}
        ]
    }]));

    assert_eq!(
        refused_fields(&arguments),
        [
            "memories[0].observed_at",
            "memories[0].labels",
            "memories[0].kind",
            "memories[0].evidence",
            "memories[0].summary_en",
            "memories[0].connect_to[0].why",
            "memories[0].connect_to[1].rel"
        ]
    );
}

#[test]
fn a_failure_found_while_reading_the_packet_supersedes_the_derived_one() {
    let arguments = packet(json!([{
        "id":"a","kind":"observation","summary":"The route opens on Tuesday.",
        "evidence":"S1 says so.","labels":{"topic":[]},
        "ref":"project:elsewhere:entry:a",
        "search_expansions":"not an array",
        "connect_to":[{"ref":"@missing","rel":"supports","why":"w","evidence":"e"}]
    }]));

    let fields = refused_fields(&arguments);
    assert_eq!(
        fields,
        [
            "memories[0].search_expansions",
            "memories[0].ref",
            "memories[0].labels",
            "memories[0].connect_to[0].ref"
        ]
    );
}

#[test]
fn a_missing_kind_or_summary_is_reported_with_the_rest_of_the_record() {
    let arguments = packet(json!([
        {"id":"a","evidence":"S1.","labels":{"topic":["x"]}},
        {"id":"b","kind":"observation","summary":"The route opens on Tuesday.","evidence":"S2."}
    ]));

    let error = ToolError::from(build_batch_plan(&arguments).expect_err("refused"));
    assert_eq!(
        error.message,
        "3 validation failures in 2 records; repair every listed field and resend the whole packet. No changes were written."
    );
    let fields = error
        .feedback
        .iter()
        .map(|item| item["field"].as_str().expect("field"))
        .collect::<Vec<_>>();
    assert_eq!(
        fields,
        [
            "memories[0].kind",
            "memories[0].summary",
            "memories[1].labels"
        ]
    );
}

#[test]
fn an_unreadable_packet_still_stops_at_once() {
    for (memories, field) in [
        (
            json!([{"id":"1a","kind":"observation","summary":"s"}]),
            "memories[0].id",
        ),
        (json!(["not a record"]), "memories[0]"),
        (
            json!([{"id":"a","kind":"observation","summary":"s"},{"id":"a","kind":"observation","summary":"t"}]),
            "memories[1].id",
        ),
    ] {
        assert_eq!(refused_fields(&packet(memories)), [field]);
    }

    let mut sequence = packet(json!([
        {"id":"a","kind":"observation","summary":"The route opens.","evidence":"S1.","labels":{"topic":["x"]}}
    ]));
    sequence["options"] = json!({"sequence": "first"});
    assert_eq!(refused_fields(&sequence), ["options.sequence"]);

    let links = packet(json!([
        {"id":"a","kind":"observation","summary":"The route opens.","evidence":"S1.",
         "labels":{"topic":["x"]},"connect_to":{"ref":"b"}}
    ]));
    assert_eq!(refused_fields(&links), ["memories[0].connect_to"]);
}

#[test]
fn a_record_whose_links_are_well_formed_and_labels_new_are_declared_plans() {
    let mut arguments = packet(json!([
        {"id":"a","kind":"observation","summary":"The route opens.","evidence":"S1.","labels":{"topic":["x"]}},
        {"id":"b","kind":"observation","summary":"The route closes.","evidence":"S2.","labels":{"topic":["y"]}}
    ]));
    arguments["options"] = json!({"labels_new": ["topic"]});
    build_batch_plan(&arguments).expect("both records plan");

    // A malformed or unknown declaration is one failure at its own field,
    // reported with the records' failures instead of stopping the packet.
    for labels_new in [json!("topic"), json!([7]), json!(["release"])] {
        let mut malformed = arguments.clone();
        malformed["options"] = json!({"labels_new": labels_new});
        malformed["memories"][1]["kind"] = json!("outcome");
        assert_eq!(
            refused_fields(&malformed),
            ["options.labels_new", "memories[1].kind"],
            "{labels_new}"
        );
    }
}
