//! Incremental continuations: the stable core travels once, on the first
//! page; later pages carry only new expansion items, and the first page's
//! core plus every continuation reconstructs the complete selection.

use kmp_proto::v1beta1::{
    AskRequest, AskResponse, MemoryBudget, MemoryDetailLevel, PageRequest, RecallCursorErrorReason,
    WakeRequest, WakeResponse,
};
use serde_json::{Value, json};

use super::plan::Section;
use super::projection_error::RecallProjectionError;
use super::response_value::{ask_value, wake_value};
use super::test_support::{
    labels_fixture, large_fixture, projected, typed_ask_fixture, typed_wake_fixture,
};
use super::typed_recall::{project_ask_response, project_wake_response};

const PAGE_BYTES: u64 = 4_000;
const WHOLE_BYTES: u64 = 1_000_000;

fn budget(max_bytes: u64) -> Option<MemoryBudget> {
    Some(MemoryBudget {
        tokens: 30_000,
        detail: MemoryDetailLevel::Full as i32,
        depth: 3,
        max_entries: 0,
        max_bytes,
    })
}

fn wake_request(max_bytes: u64) -> WakeRequest {
    WakeRequest {
        about: "project:kmp".to_string(),
        role: "implementer".to_string(),
        intent: "continue parity work".to_string(),
        budget: budget(max_bytes),
        ..Default::default()
    }
}

fn ask_request(max_bytes: u64) -> AskRequest {
    AskRequest {
        about: "project:kmp".to_string(),
        question: "Which storage engine is current?".to_string(),
        budget: budget(max_bytes),
        ..Default::default()
    }
}

fn wake_fixture() -> WakeResponse {
    let mut response = typed_wake_fixture(24);
    response.labels = labels_fixture(12);
    response.warnings = vec!["a stored-scope warning from the query".to_string()];
    response
}

fn ask_fixture() -> AskResponse {
    let mut response = typed_ask_fixture(24);
    response.warnings = vec!["a stored-scope warning from the query".to_string()];
    response
}

fn page_request(cursor: &str, repeat_core: bool) -> Option<PageRequest> {
    Some(PageRequest {
        entries: 0,
        cursor: cursor.to_string(),
        repeat_core,
    })
}

fn next_cursor(value: &Value) -> Option<String> {
    value
        .pointer("/projection/page/next_cursor")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

/// Everything a page says about the memory, without its own bookkeeping.
fn content(value: &Value) -> Value {
    let mut value = value.clone();
    let object = value.as_object_mut().expect("page object");
    object.remove("projection");
    object.remove("truncation");
    object.remove("warnings");
    value
}

/// Append every expansion array of `page` onto `packet`, in section order.
fn append_items(packet: &mut Value, page: &Value) {
    for section in Section::ALL {
        let Some(items) = page
            .pointer(&format!("/{}", section.path().join("/")))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let mut target = &mut *packet;
        for key in section.path() {
            target = target
                .get_mut(*key)
                .unwrap_or_else(|| panic!("first page carries {}", section.name()));
        }
        target
            .as_array_mut()
            .expect("section array")
            .extend(items.iter().cloned());
    }
}

/// A continuation carries only expansion arrays, the progress block and its
/// own warnings; nothing of the stable core.
fn assert_incremental(page: &Value) {
    assert_eq!(page["projection"]["core_reused"], true, "{page}");
    let object = page.as_object().expect("page object");
    for key in object.keys() {
        assert!(
            matches!(
                key.as_str(),
                "wake" | "proof" | "labels" | "projection" | "warnings"
            ),
            "continuation repeats core field `{key}`: {page}"
        );
    }
    for (parent, child) in [
        ("wake", &["objective"][..]),
        ("proof", &["confidence", "conflicts", "matched_terms"][..]),
    ] {
        for key in child {
            assert!(
                page.get(parent).and_then(|value| value.get(*key)).is_none(),
                "continuation repeats {parent}.{key}: {page}"
            );
        }
    }
    let warnings = page["warnings"].as_array().expect("warnings");
    assert!(
        !warnings
            .iter()
            .any(|warning| warning == "a stored-scope warning from the query"),
        "core warnings travel with the core: {page}"
    );
}

/// Walk a selection page by page, then rebuild it from page 1's core plus the
/// continuation items, and compare it with a single large-budget read.
fn walk_and_reconstruct(first: Value, mut next: impl FnMut(&str) -> Value, whole: Value) -> usize {
    assert!(first["projection"].get("core_reused").is_none());
    assert_eq!(first["projection"]["core_text_shortened"], false);
    let mut packet = content(&first);
    let mut cursor = next_cursor(&first).expect("the fixture needs more than one page");
    let mut pages = 1usize;
    loop {
        let page = next(&cursor);
        assert_incremental(&page);
        append_items(&mut packet, &page);
        pages += 1;
        assert!(pages < 40, "continuations must make progress");
        match next_cursor(&page) {
            Some(following) => cursor = following,
            None => {
                assert!(page["projection"]["next_action"].is_null());
                break;
            }
        }
    }
    assert_eq!(packet, content(&whole));
    for warning in whole["warnings"].as_array().expect("warnings") {
        assert!(
            first["warnings"]
                .as_array()
                .expect("warnings")
                .contains(warning),
            "page 1 keeps the core warning {warning}"
        );
    }
    pages
}

#[test]
fn wake_continuations_omit_the_core_and_reconstruct_the_single_read() {
    let response = wake_fixture();
    let whole = wake_value(
        &project_wake_response(response.clone(), &wake_request(WHOLE_BYTES)).expect("whole wake"),
    );
    assert!(whole["projection"]["page"]["has_more"] == false);
    let first = project_wake_response(response.clone(), &wake_request(PAGE_BYTES))
        .expect("first wake page");
    let pages = walk_and_reconstruct(
        wake_value(&first),
        |cursor| {
            let mut request = wake_request(PAGE_BYTES);
            request.page = page_request(cursor, false);
            let page = project_wake_response(response.clone(), &request).expect("wake page");
            assert!(page.projection.as_ref().expect("projection").core_reused);
            assert!(page.summary.is_empty());
            assert!(page.resume_cursor.is_none());
            let value = wake_value(&page);
            let used = page
                .projection
                .as_ref()
                .and_then(|projection| projection.budget)
                .expect("budget")
                .used_bytes;
            assert_eq!(
                used,
                serde_json::to_vec(&value).expect("bytes").len() as u64,
                "used_bytes is the continuation the caller receives"
            );
            assert!(used <= PAGE_BYTES);
            value
        },
        whole,
    );
    assert!(pages > 1);
}

#[test]
fn ask_continuations_omit_the_core_and_reconstruct_the_single_read() {
    let response = ask_fixture();
    let whole = ask_value(
        &project_ask_response(response.clone(), &ask_request(WHOLE_BYTES)).expect("whole ask"),
    );
    let first =
        project_ask_response(response.clone(), &ask_request(PAGE_BYTES)).expect("first ask page");
    let pages = walk_and_reconstruct(
        ask_value(&first),
        |cursor| {
            let mut request = ask_request(PAGE_BYTES);
            request.page = page_request(cursor, false);
            let page = project_ask_response(response.clone(), &request).expect("ask page");
            assert!(page.answer.is_empty());
            assert!(page.because.is_empty());
            ask_value(&page)
        },
        whole,
    );
    assert!(pages > 1);
}

#[test]
fn json_continuations_reconstruct_the_single_read() {
    let packet = large_fixture(40);
    let arguments = |max_bytes: u64| {
        json!({
            "about": "project:kmp",
            "question": "Which storage engine is current?",
            "budget": {"tokens": 30_000, "max_bytes": max_bytes, "detail": "full"}
        })
    };
    let whole = projected(packet.clone(), arguments(WHOLE_BYTES));
    let first = projected(packet.clone(), arguments(4_000));
    walk_and_reconstruct(
        first,
        |cursor| {
            let mut next = arguments(4_000);
            next["page"] = json!({"cursor": cursor});
            projected(packet.clone(), next)
        },
        whole,
    );
}

#[test]
fn repeat_core_rehydrates_the_same_items_with_the_core() {
    let response = wake_fixture();
    let first = project_wake_response(response.clone(), &wake_request(PAGE_BYTES))
        .expect("first wake page");
    let first_value = wake_value(&first);
    let cursor = next_cursor(&first_value).expect("cursor");

    let mut incremental = wake_request(PAGE_BYTES);
    incremental.page = page_request(&cursor, false);
    let incremental = wake_value(
        &project_wake_response(response.clone(), &incremental).expect("incremental page"),
    );

    let mut rehydrate = wake_request(PAGE_BYTES);
    rehydrate.page = page_request(&cursor, true);
    let rehydrated = project_wake_response(response, &rehydrate).expect("rehydrated page");
    assert!(
        !rehydrated
            .projection
            .as_ref()
            .expect("projection")
            .core_reused
    );
    let rehydrated = wake_value(&rehydrated);
    assert!(rehydrated["projection"].get("core_reused").is_none());
    for field in ["summary", "scope", "resume_cursor"] {
        assert_eq!(rehydrated[field], first_value[field], "{field}");
    }
    assert_eq!(
        rehydrated["wake"]["objective"],
        first_value["wake"]["objective"]
    );
    assert_eq!(
        rehydrated["proof"]["confidence"],
        first_value["proof"]["confidence"]
    );
    assert!(
        rehydrated["warnings"]
            .as_array()
            .expect("warnings")
            .contains(&json!("a stored-scope warning from the query"))
    );
    // The same cursor names the same next items; the core adds bytes, so
    // the rehydrated page can hold a shorter prefix of them, never others.
    let incremental_offset = &incremental["projection"]["page"]["offset"];
    assert_eq!(
        &rehydrated["projection"]["page"]["offset"],
        incremental_offset
    );
    let rehydrated_returned = rehydrated["projection"]["page"]["returned"]
        .as_u64()
        .expect("returned");
    assert!(rehydrated_returned > 0);
    assert!(
        rehydrated_returned
            <= incremental["projection"]["page"]["returned"]
                .as_u64()
                .expect("returned")
    );
    // A rehydration is one call: the action it proposes goes back to
    // incremental pages.
    if let Some(action) = rehydrated["projection"]["next_action"].as_object() {
        assert!(action["arguments"]["page"].get("repeat_core").is_none());
    }
}

#[test]
fn an_incremental_cursor_still_rejects_a_changed_snapshot() {
    let response = wake_fixture();
    let first = project_wake_response(response.clone(), &wake_request(PAGE_BYTES))
        .expect("first wake page");
    let cursor = next_cursor(&wake_value(&first)).expect("cursor");
    let mut changed = response;
    changed.proof.as_mut().expect("proof").evidence[5].text = "A later snapshot.".to_string();
    for repeat_core in [false, true] {
        let mut request = wake_request(PAGE_BYTES);
        request.page = page_request(&cursor, repeat_core);
        let error = project_wake_response(changed.clone(), &request)
            .expect_err("a cursor never mixes snapshots");
        assert!(
            matches!(
                error,
                RecallProjectionError::Cursor {
                    reason: RecallCursorErrorReason::SelectionChanged,
                    ..
                }
            ),
            "{error}"
        );
    }
}

#[test]
fn a_first_page_always_carries_the_core() {
    let packet = large_fixture(40);
    let page = projected(
        packet,
        json!({
            "about": "project:kmp",
            "question": "Which storage engine is current?",
            "budget": {"max_bytes": 4_000},
            "page": {"repeat_core": false}
        }),
    );
    assert!(page["answer"].is_string());
    assert!(page["projection"].get("core_reused").is_none());
}

#[test]
fn projection_is_the_one_progress_block() {
    let packet = large_fixture(40);
    let arguments = json!({
        "about": "project:kmp",
        "question": "Which storage engine is current?",
        "budget": {"max_bytes": 4_000}
    });
    let first = projected(packet.clone(), arguments.clone());
    assert!(first.get("truncation").is_none(), "{first}");
    let mut next = arguments;
    next["page"] = json!({"cursor": next_cursor(&first).expect("cursor")});
    let second = projected(packet, next);
    assert!(second.get("truncation").is_none(), "{second}");
    // Every omission cause stays distinguishable in `projection`.
    for pointer in [
        "/projection/page/offset",
        "/projection/page/returned",
        "/projection/page/total",
        "/projection/page/has_more",
        "/projection/excluded_by_detail",
        "/projection/selection_omitted",
        "/projection/core_text_shortened",
        "/projection/budget/max_bytes",
        "/projection/budget/tokens_advisory",
    ] {
        assert!(second.pointer(pointer).is_some(), "{pointer}: {second}");
    }
}

#[test]
fn repeat_core_must_be_a_boolean() {
    let error = super::recall_output::project_recall_output(
        large_fixture(4),
        &json!({"about": "project:kmp", "question": "q", "page": {"repeat_core": "yes"}}),
        2_400,
        &kmp_application::queries::cl100k_estimator::Cl100kEstimator::new(),
    )
    .expect_err("repeat_core is a boolean");
    assert!(error.contains("page.repeat_core"), "{error}");
}
