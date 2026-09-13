//! What a core that must be shortened still guarantees: one identical core
//! across detail tiers, and a page that can always advance.

use std::collections::BTreeSet;

use serde_json::json;

use super::core_fit::serialized_bytes;
use super::test_support::{fixture, large_fixture, projected};

#[test]
fn oversized_expansion_explains_stall_and_resumes_at_larger_budget() {
    let mut packet = fixture();
    packet["proof"]["evidence"][1]["text"] = json!("large literal source ".repeat(800));
    let mut args = json!({
        "about": "project:kmp", "question": "What is current?",
        "budget": {"max_bytes": 10_000, "detail": "full"},
        "page": {"entries": 1}
    });
    // The causal path precedes the oversized evidence, giving a real
    // nonzero continuation offset rather than only a stalled first page.
    let first = projected(packet.clone(), args.clone());
    assert_eq!(first["projection"]["page"]["returned"], 1);
    let cursor = first["projection"]["page"]["next_cursor"].clone();
    args["page"]["cursor"] = cursor.clone();
    let stalled = projected(packet.clone(), args.clone());
    assert!(serialized_bytes(&stalled) <= 10_000);
    assert_eq!(stalled["projection"]["page"]["offset"], 1);
    assert_eq!(stalled["projection"]["page"]["returned"], 0);
    assert_eq!(stalled["projection"]["page"]["has_more"], true);
    assert_eq!(stalled["projection"]["page"]["next_cursor"], cursor);
    assert_eq!(stalled["because"], packet["because"]);
    assert!(
        stalled["projection"]["next_action"]["arguments"]["budget"]["max_bytes"]
            .as_u64()
            .expect("proposed allowance")
            > 10_000
    );
    assert!(
        stalled["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|w| w
                .as_str()
                .is_some_and(|w| w.contains("discard the partial reconstruction")))
    );
    assert_eq!(stalled["projection"]["core_text_shortened"], true);
    let restoration = stalled["projection"]["next_action"]["arguments"].clone();
    assert!(restoration.pointer("/page/cursor").is_none());
    let restored = projected(packet.clone(), restoration);
    assert_eq!(restored["projection"]["core_text_shortened"], false);
    assert_eq!(restored["projection"]["page"]["offset"], 0);

    args["budget"]["max_bytes"] = json!(30_000);
    let resumed = projected(packet.clone(), args);
    assert_eq!(resumed["projection"]["page"]["offset"], 1);
    assert_eq!(resumed["projection"]["page"]["returned"], 1);
    assert_ne!(resumed["projection"]["page"]["next_cursor"], cursor);
    assert!(
        resumed["proof"]["evidence"]
            .as_array()
            .expect("resumed evidence")
            .contains(&packet["proof"]["evidence"][1])
    );
}

#[test]
fn compact_1400_and_balanced_1800_keep_the_same_cited_core() {
    let packet = large_fixture(80);
    let compact = projected(
        packet.clone(),
        json!({
            "about": "project:kmp",
            "question": "Which storage engine is current?",
            "budget": {"tokens": 1400, "max_bytes": 10_000, "detail": "compact"}
        }),
    );
    let balanced = projected(
        packet,
        json!({
            "about": "project:kmp",
            "question": "Which storage engine is current?",
            "budget": {"tokens": 1800, "max_bytes": 10_000, "detail": "balanced"}
        }),
    );

    assert_eq!(compact["answer"], balanced["answer"]);
    assert_eq!(compact["because"], balanced["because"]);
    let compact_evidence = compact["proof"]["evidence"].as_array().expect("evidence");
    let balanced_evidence = balanced["proof"]["evidence"].as_array().expect("evidence");
    assert!(balanced_evidence.starts_with(compact_evidence));
    assert_eq!(compact["because"].as_array().expect("reasons").len(), 3);
}

#[test]
fn core_is_identical_across_detail_modes_when_text_must_shorten() {
    let packet = large_fixture(30);
    let args = |detail: &str| {
        json!({
            "about": "project:kmp",
            "question": "Which storage engine is current?",
            "budget": {"tokens": 900, "max_bytes": 2_200, "detail": detail}
        })
    };
    let compact = projected(packet.clone(), args("compact"));
    let balanced = projected(packet.clone(), args("balanced"));
    let full = projected(packet, args("full"));

    assert_eq!(compact["projection"]["core_text_shortened"], true);
    for field in ["answer", "because"] {
        assert_eq!(compact[field], balanced[field]);
        assert_eq!(balanced[field], full[field]);
    }
    // The stable core is identical. Expansion can differ by detail and
    // how many eligible items fit beside each tier's metadata.
    for field in ["evidence", "path"] {
        let name = format!("proof.{field}");
        let core = compact["projection"]["sections"][&name]["core"]
            .as_u64()
            .expect("core count") as usize;
        for output in [&balanced, &full] {
            assert_eq!(output["projection"]["sections"][&name]["core"], core);
            assert_eq!(
                &compact["proof"][field].as_array().expect("proof section")[..core],
                &output["proof"][field].as_array().expect("proof section")[..core]
            );
        }
    }
    for output in [&compact, &balanced, &full] {
        assert!(
            output["projection"]["page"]["returned"].as_u64() > Some(0),
            "a shortened core must still leave room for paging progress"
        );
    }
}

#[test]
fn all_abouts_wake_with_a_shortened_core_advances_every_page() {
    let mut packet = large_fixture(30);
    packet
        .as_object_mut()
        .expect("wake packet")
        .remove("answer");
    packet
        .as_object_mut()
        .expect("wake packet")
        .remove("because");
    packet["wake"] = json!({
        "objective": "Sweep every memory anchor.",
        "current_state": [
            format!("Cross-project state: {}", "stable core detail ".repeat(240)),
            "Second project state",
            "Third project state"
        ],
        "causal_spine": [{
            "claim": "claim:0",
            "because": "The first evidence item anchors the sweep.",
            "evidence_ref": "evidence:0"
        }],
        "open_loops": ["Inspect every remaining anchor"],
        "next_actions": ["Continue with the returned cursor"],
        "guardrails": ["Never report a partial sweep as complete"]
    });
    let base_arguments = json!({
        "about": "project:kmp",
        "dimensions": {"scope": "all_abouts"},
        "budget": {"max_bytes": 4_000, "detail": "full"},
        "page": {"entries": 4}
    });

    let mut cursor = None;
    let mut expected_offset = 0_u64;
    let mut seen_cursors = BTreeSet::new();
    loop {
        let mut arguments = base_arguments.clone();
        if let Some(cursor) = &cursor {
            arguments["page"]["cursor"] = json!(cursor);
        }
        let output = projected(packet.clone(), arguments);
        let page = &output["projection"]["page"];
        let returned = page["returned"].as_u64().expect("returned");
        assert_eq!(page["offset"], expected_offset);
        assert!(
            returned > 0,
            "every continuation must make progress: {output}"
        );
        assert_eq!(output["projection"]["core_text_shortened"], true);
        assert!(
            serde_json::to_vec(&output).expect("projection bytes").len() <= 4_000,
            "the progress guarantee must preserve the byte ceiling"
        );
        expected_offset += returned;

        if page["has_more"] == false {
            assert!(page["next_cursor"].is_null());
            assert_eq!(page["total"], expected_offset);
            break;
        }
        let next = page["next_cursor"]
            .as_str()
            .expect("continuation cursor")
            .to_string();
        assert!(
            seen_cursors.insert(next.clone()),
            "a continuation cursor must never repeat"
        );
        cursor = Some(next);
        assert!(seen_cursors.len() < 100, "the fixture must terminate");
    }
}
