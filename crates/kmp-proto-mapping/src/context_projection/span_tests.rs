use super::*;
use sha2::{Digest, Sha256};

fn group(id: &str, text: &str) -> ContextGroup {
    ContextGroup {
        id: id.into(),
        packets: vec![
            json!({"object":{"ref":id,"text":text,"metadata":{"author":id,"source_kind":"original","observed_at":"2026-09-08T10:00:00Z"}},"warnings":[]}),
        ],
        reads: vec![json!({"tool":"kmp_inspect","arguments":{"about":"project:spans","ref":id}})],
        spans: vec![],
    }
}

fn fixture() -> Vec<ContextGroup> {
    let text = "🙂 R7 is permitted only on the local device. R8 is not authorized. No completion until both checks are recorded. ".repeat(40);
    let first = group("source:A", &text);
    let independent = group("source:B", &text);
    let mut quote = group(
        "derived:A",
        "The plan cites only source A; its partial checks do not establish completion.",
    );
    quote.packets[0]["evidence"] = json!([
        {"id":"quote:1","source":"source:A","text":text.get(0..2200).expect("boundary"),"supports":["derived:A"]},
        {"id":"quote:2","source":"source:A","text":text.get(1100..3300).expect("boundary"),"supports":["derived:A"]}
    ]);
    quote.packets[0]["object"]["metadata"]["source_kind"] = json!("derived");
    quote.packets[0]["proof"] = json!({"missing":["manifest check"],"conflicts":[{"from":"source:A","to":"conflicting:source"}]});
    for (index, start, end) in [(0, 0, 2200), (1, 1100, 3300)] {
        quote.spans.push(SourceSpan {
            packet: 0,
            pointer: format!("/evidence/{index}/text"),
            source_ref: "source:A".into(),
            source_sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
            start_utf8: start,
            end_utf8: end,
        });
    }
    vec![first, independent, quote]
}

#[test]
fn checked_overlaps_share_the_returned_source_without_merging_attestations() {
    let groups = fixture();
    let before = json!(&groups);
    let projected = compose(&groups, usize::MAX).expect("verified ranges");
    let quote = &projected["groups"][2]["packets"][0]["evidence"][0]["text"];
    assert_eq!(
        quote["passage"]
            .as_array()
            .expect("ordered fragments")
            .len(),
        2
    );
    assert_eq!(
        projected["groups"][0]["packets"][0]["object"]["ref"],
        "source:A"
    );
    assert_eq!(
        projected["groups"][1]["packets"][0]["object"]["metadata"]["author"],
        "source:B"
    );
    let mut exact_only = groups.clone();
    exact_only[2].spans.clear();
    assert!(
        projected.to_string().len()
            < compose(&exact_only, usize::MAX)
                .expect("plain")
                .to_string()
                .len()
    );
    assert_eq!(expand(&projected).expect("full reconstruction"), groups);
    assert_eq!(json!(&groups), before, "input never mutated");
    assert_eq!(compose(&groups, usize::MAX).expect("replay"), projected);
}

#[test]
fn a_source_omitted_by_budget_cannot_reenter_via_a_slice_table() {
    let text = "Only R7 is authorized. No manifest check is recorded. ".repeat(300);
    let source = group("source", &text);
    let mut quote = group("quote", &text[..51]);
    quote.spans.push(SourceSpan {
        packet: 0,
        pointer: "/object/text".into(),
        source_ref: "source".into(),
        source_sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
        start_utf8: 0,
        end_utf8: 51,
    });
    let result = compose(&[source, quote.clone()], 1500).expect("budget");
    assert_eq!(expand(&result).expect("quote stays whole"), vec![quote]);
    assert_eq!(
        result["groups"][0]["packets"][0]["object"]["text"],
        &text[..51]
    );
    assert_eq!(result["omitted"][0]["id"], "source");
    assert_eq!(result["omitted"][0]["reason"], "byte_budget");
    assert_eq!(
        result["omitted"][0]["reads"][0]["arguments"]["ref"],
        "source"
    );
}

#[test]
fn source_spans_reject_wrong_provenance_boundaries_slots_and_duplicate_bindings() {
    for failure in 0..9 {
        let mut groups = fixture();
        let span = &mut groups[2].spans[0];
        match failure {
            0 => span.source_ref = "absent".into(),
            1 => span.source_sha256 = "0".repeat(64),
            2 => span.pointer = "/object/metadata/author".into(),
            3 => span.start_utf8 = 1, // inside the emoji
            4 => span.end_utf8 = 99999,
            5 => span.packet = 1,
            6 => span.end_utf8 = 0,
            7 => span.end_utf8 -= 1, // not the complete quote
            _ => {
                let duplicate = span.clone();
                groups[2].spans.push(duplicate);
            }
        }
        assert!(compose(&groups, usize::MAX).is_err(), "case {failure}");
    }
}

#[test]
fn admission_preserves_whole_groups_qualifiers_conflicts_and_all_omissions() {
    let groups = fixture();
    let ceiling = compose(&groups, usize::MAX)
        .expect("full")
        .to_string()
        .len();
    for percent in [100, 75, 50] {
        let result = compose(&groups, ceiling * percent / 100).expect("bounded");
        let expanded = expand(&result).expect("admitted");
        for group in &expanded {
            assert_eq!(
                group,
                groups
                    .iter()
                    .find(|old| old.id == group.id)
                    .expect("original")
            );
        }
        assert_eq!(
            expanded.len() + result["omitted"].as_array().expect("omissions").len(),
            groups.len()
        );
        assert!(
            result.to_string().len() <= ceiling * percent / 100
                || !result["warnings"].as_array().expect("warnings").is_empty()
        );
    }
}

#[test]
fn response_local_tables_are_expanded_per_packet_before_context_composition() {
    let groups = [
        group("a", &"No R8 permission. ".repeat(30)),
        group("b", &"R7 checksum remains unknown. ".repeat(30)),
    ];
    let mut encoded = groups.clone();
    for group in &mut encoded {
        let text = group.packets[0]["object"]["text"].take();
        group.packets[0]["object"]["text"] = json!({"passage":"p1"});
        group.packets[0]["passages"] = json!({"p1":text});
    }
    assert_eq!(
        expand(&compose(&encoded, usize::MAX).expect("compose pages")).expect("canonical"),
        groups
    );
}

#[test]
fn invalid_fragment_lists_and_unsupported_context_versions_fail_explicitly() {
    let mut result = compose(&fixture(), usize::MAX).expect("source ranges");
    for invalid in [
        json!({"passage":[]}),
        json!({"passage":["p1", 2]}),
        json!({"passage":["p1", "absent"]}),
        json!({"passage":null}),
        json!({"passage":"p1", "start_utf8":1}),
    ] {
        let mut malformed = result.clone();
        malformed["groups"][2]["packets"][0]["evidence"][0]["text"] = invalid;
        assert!(expand(&malformed).is_err());
    }
    result["contract"] = json!("kmp.context.passages.v1");
    assert!(expand(&result).is_err());
    for value in [
        json!({"groups":[],"max_bytes":-1}),
        json!({"groups":[],"max_bytes":"1000"}),
        json!({"groups":[],"budget":1000}),
    ] {
        assert!(serde_json::from_value::<ProjectionRequest>(value).is_err());
    }
}
