//! Every recall page reports what it still owes, through the typed wire.
use std::collections::{BTreeMap, BTreeSet};

use kmp_proto::v1beta1::{
    AnswerReason, AskRequest, AskResponse, MemoryConfidence, MemoryDetailLevel, MemoryEvidence,
    MemoryRelation, MemorySemanticClass, WakeRequest, WakeResponse,
};
use serde_json::{Value, json};

use super::core_fit::serialized_bytes;
use super::response_value::{ask_value, wake_value};
use super::typed_recall::{project_ask_response, project_wake_response};

fn response() -> AskResponse {
    AskResponse {
        answer: "Stored assertion; supporting passages still need review.".into(),
        because: vec![AnswerReason {
            claim: "claim:0".into(),
            r#ref: "evidence:0".into(),
            ..Default::default()
        }],
        proof: Some(kmp_proto::v1beta1::Proof {
            confidence: MemoryConfidence::High as i32,
            evidence: (0..6)
                .map(|n| MemoryEvidence {
                    support_clocks: None,
                    id: format!("evidence:{n}"),
                    supports: vec![format!("claim:{n}")],
                    text: format!(
                        "Literal evidence {n}; neither labels nor pagination prove identity."
                    ),
                    source: format!("source:{n}"),
                    ..Default::default()
                })
                .collect(),
            path: (1..6)
                .map(|n| MemoryRelation {
                    source_ref: format!("claim:{n}"),
                    target_ref: "claim:0".into(),
                    rel: "depends_on".into(),
                    semantic_class: MemorySemanticClass::Causal as i32,
                    confidence: MemoryConfidence::High as i32,
                    why: format!("Source {n} declares a prerequisite."),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn request() -> AskRequest {
    AskRequest {
        about: "project:kmp".into(),
        question: "Which prerequisites were recorded?".into(),
        budget: Some(kmp_proto::v1beta1::MemoryBudget {
            max_bytes: 20_000,
            detail: MemoryDetailLevel::Full as i32,
            ..Default::default()
        }),
        page: Some(kmp_proto::v1beta1::PageRequest {
            entries: 2,
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Per section: (core, eligible, returned on earlier pages). Counters are
/// lean: a zero is omitted, and `eligible` is derived from page 1, which
/// states `core` and leaves nothing on earlier pages.
type Progress = BTreeMap<String, (u64, u64, u64)>;

fn check_progress(value: &Value, previous: &mut Progress) {
    let projection = &value["projection"];
    // A continuation carries only its new items; page 1 alone holds the core.
    let core_reused = projection["core_reused"] == true;
    let count = |section: &Value, key: &str| section.get(key).and_then(Value::as_u64).unwrap_or(0);
    let sections = projection["sections"].as_object().expect("sections");
    if !core_reused {
        assert!(previous.is_empty(), "only page 1 carries the core here");
        for (name, section) in sections {
            let core = count(section, "core");
            let eligible = core + count(section, "returned_on_page") + count(section, "remaining");
            previous.insert(name.clone(), (core, eligible, 0));
        }
    }
    let mut pending = 0;
    for (name, (core, eligible, earlier)) in previous.iter_mut() {
        let empty = json!({});
        let section = sections.get(name).unwrap_or(&empty);
        let returned = count(section, "returned_on_page");
        let remaining = count(section, "remaining");
        assert_eq!(*core + *earlier + returned + remaining, *eligible, "{name}");
        let pointer = format!("/{}", name.replace('.', "/"));
        let carried = value
            .pointer(&pointer)
            .and_then(Value::as_array)
            .map_or(0, Vec::len) as u64;
        assert_eq!(
            carried,
            if core_reused {
                returned
            } else {
                *core + returned
            }
        );
        *earlier += returned;
        pending += remaining;
    }
    for name in sections.keys() {
        assert!(previous.contains_key(name), "{name} appeared after page 1");
    }
    let page = &projection["page"];
    assert_eq!(
        pending,
        page["total"].as_u64().expect("section count")
            - page["offset"].as_u64().expect("section count")
            - page["returned"].as_u64().expect("section count")
    );
    assert_eq!(pending > 0, page["has_more"] == true);
    assert_eq!(
        serialized_bytes(value) as u64,
        projection["budget"]["used_bytes"]
    );
    assert!(serialized_bytes(value) <= 20_000);
}

#[test]
fn ask_remaining_excludes_core_and_every_prior_page_through_typed_wire() {
    use prost::Message;
    let response = response();
    let mut request = request();
    let mut prior = BTreeMap::new();
    let mut seen = BTreeSet::new();
    loop {
        let page = project_ask_response(response.clone(), &request).expect("page");
        let decoded = AskResponse::decode(page.encode_to_vec().as_slice()).expect("wire");
        let value = ask_value(&decoded);
        assert_eq!(value, ask_value(&page));
        check_progress(&value, &mut prior);
        let meta = decoded
            .projection
            .expect("projection")
            .page
            .expect("page accounting");
        if !meta.has_more {
            break;
        }
        let cursor = meta.next_cursor.expect("continuation cursor");
        assert!(seen.insert(cursor.clone()), "must advance");
        request.page.as_mut().expect("page request").cursor = cursor;
    }
    assert!(seen.len() > 1);
    assert_eq!(prior["proof.evidence"].2, 5);
}

#[test]
fn wake_remaining_counts_only_expansion_after_the_current_page() {
    let response = WakeResponse {
        summary: "Stored project status.".into(),
        proof: response().proof,
        ..Default::default()
    };
    let base = request();
    let mut request = WakeRequest {
        about: base.about,
        budget: base.budget,
        page: base.page,
        ..Default::default()
    };
    let mut prior = BTreeMap::new();
    let mut pages = 0;
    loop {
        let page = project_wake_response(response.clone(), &request).expect("page");
        check_progress(&wake_value(&page), &mut prior);
        let meta = page
            .projection
            .expect("projection")
            .page
            .expect("page accounting");
        pages += 1;
        assert!(pages < 30, "must terminate");
        if !meta.has_more {
            break;
        }
        request.page.as_mut().expect("page request").cursor =
            meta.next_cursor.expect("continuation cursor");
    }
    assert!(pages > 1);
}

#[test]
fn zero_remaining_does_not_erase_detail_exclusions() {
    let mut request = request();
    request.budget.as_mut().expect("budget").detail = MemoryDetailLevel::Compact as i32;
    request.page = None;
    let page = project_ask_response(response(), &request).expect("page");
    let projection = page.projection.expect("projection");
    assert!(
        projection
            .sections
            .iter()
            .all(|section| section.remaining == 0)
    );
    assert!(projection.excluded_by_detail > 0);
    assert!(!projection.page.expect("page accounting").has_more);
}

#[test]
fn shortened_core_warns_to_restart_even_with_pending_expansion() {
    let mut request = request();
    request.budget.as_mut().expect("budget").max_bytes = 512;
    let page = project_ask_response(response(), &request).expect("floor");
    let value = ask_value(&page);
    assert_eq!(value["projection"]["core_text_shortened"], true);
    assert!(
        value
            .pointer("/projection/next_action/arguments/page/cursor")
            .is_none()
    );
    assert!(
        page.warnings
            .iter()
            .any(|w| w.contains("discard the partial reconstruction"))
    );
    assert!(
        value["projection"]["sections"]
            .as_object()
            .expect("section map")
            .values()
            .any(|s| s["remaining"].as_u64().expect("section count") > 0)
    );
}
