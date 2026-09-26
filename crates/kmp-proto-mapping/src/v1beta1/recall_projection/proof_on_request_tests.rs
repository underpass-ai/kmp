//! An answered ask keeps the rest of its proof on request: its first page
//! offers no continuation below `full` detail and says how much it withheld.

use kmp_proto::v1beta1::{AnswerStatus, AskRequest, MemoryBudget, MemoryDetailLevel, PageRequest};
use serde_json::{Value, json};

use super::proof_on_request::MORE_ON_REQUEST;
use super::response_value::ask_value;
use super::test_support::{large_fixture, projected, typed_ask_fixture};
use super::typed_recall::project_ask_response;

const PAGE_BYTES: u64 = 4_000;
const RESTART_WARNING: &str = "recall core prose was shortened; execute projection.next_action to restart and discard the partial reconstruction before reading more expansion";

fn with_status(status: &str) -> Value {
    let mut value = large_fixture(24);
    value["answer_status"] = json!(status);
    value
}

fn arguments(detail: &str) -> Value {
    json!({"about": "project:kmp", "question": "q", "budget": {"max_bytes": PAGE_BYTES, "detail": detail}})
}

fn projection(value: &Value) -> &Value {
    value.get("projection").expect("projection")
}

#[test]
fn an_answered_first_page_offers_no_continuation_and_counts_what_it_withheld() {
    for detail in ["compact", "balanced"] {
        let value = projected(with_status("answered"), arguments(detail));
        let projection = projection(&value);
        assert_eq!(projection["next_action"], Value::Null, "{detail}");
        assert_eq!(projection["page"]["has_more"], false, "{detail}");
        assert_eq!(projection["page"]["next_cursor"], Value::Null, "{detail}");
        if detail == "compact" {
            // Compact leaves few items eligible: whether any were withheld
            // depends on the fixture, never that a continuation is offered.
            continue;
        }
        let more = projection["more_on_request"]
            .as_u64()
            .expect("more_on_request");
        assert!(more > 0, "{detail}: the fixture does not fit one page");
        // Nothing is left pending: every section's rest is on request.
        for (name, section) in projection["sections"].as_object().expect("sections") {
            assert!(
                section.get("remaining").is_none(),
                "{detail}: {name} still pending"
            );
        }
        let warnings = value["warnings"].as_array().expect("warnings");
        assert!(warnings.iter().any(|w| w == MORE_ON_REQUEST), "{detail}");
    }
}

#[test]
fn the_withheld_count_is_what_an_unsettled_reading_would_have_paged() {
    let answered = projected(with_status("answered"), arguments("balanced"));
    let partial = projected(with_status("partial"), arguments("balanced"));
    let returned = |value: &Value| projection(value)["page"]["returned"].as_u64().unwrap();
    let total = projection(&partial)["page"]["total"].as_u64().unwrap();
    assert_eq!(
        returned(&answered) + projection(&answered)["more_on_request"].as_u64().unwrap(),
        total
    );
}

#[test]
fn full_detail_pages_an_answered_reading_as_before() {
    let value = projected(with_status("answered"), arguments("full"));
    let projection = projection(&value);
    assert!(projection.get("more_on_request").is_none());
    assert_eq!(projection["page"]["has_more"], true);
    assert!(projection["next_action"].is_object());
}

#[test]
fn partial_unknown_and_ungated_readings_keep_their_continuation() {
    for value in [
        with_status("partial"),
        with_status("unknown"),
        large_fixture(24),
    ] {
        let page = projected(value, arguments("balanced"));
        let projection = projection(&page);
        assert!(projection.get("more_on_request").is_none());
        assert_eq!(projection["page"]["has_more"], true);
        assert!(projection["next_action"].is_object());
    }
}

#[test]
fn an_answered_reading_that_fits_one_page_says_nothing_more() {
    let arguments =
        json!({"about": "project:kmp", "question": "q", "budget": {"max_bytes": 1_000_000}});
    let value = projected(with_status("answered"), arguments);
    let projection = projection(&value);
    assert!(projection.get("more_on_request").is_none());
    assert_eq!(projection["page"]["has_more"], false);
    assert_eq!(projection["next_action"], Value::Null);
}

#[test]
fn the_answered_core_is_the_core_of_the_same_reading_unanswered() {
    // Withholding changes what follows the core, never the core: the
    // planning envelope reserves the counter for any answered ask.
    let core = |value: &Value| {
        let mut value = value.clone();
        for key in ["projection", "warnings", "answer_status"] {
            value.as_object_mut().unwrap().remove(key);
        }
        for section in ["evidence", "path", "missing"] {
            value["proof"].as_object_mut().unwrap().remove(section);
        }
        value
    };
    let answered = projected(with_status("answered"), arguments("balanced"));
    let partial = projected(with_status("partial"), arguments("balanced"));
    assert_eq!(core(&answered), core(&partial));
    assert_eq!(answered["because"], partial["because"]);
}

#[test]
fn an_explicit_cursor_on_an_answered_reading_is_served_as_a_page() {
    let full = projected(with_status("answered"), arguments("full"));
    let cursor = projection(&full)["page"]["next_cursor"]
        .as_str()
        .expect("full detail pages")
        .to_string();
    let mut args = arguments("full");
    args["page"] = json!({"cursor": cursor});
    let page = projected(with_status("answered"), args);
    assert!(projection(&page).get("more_on_request").is_none());
    assert!(projection(&page)["page"]["returned"].as_u64().unwrap() > 0);
}

#[test]
fn the_typed_answer_carries_the_counter_through_the_proto() {
    let mut response = typed_ask_fixture(24);
    response.answer_status = AnswerStatus::Answered as i32;
    let request = AskRequest {
        about: "project:kmp".to_string(),
        question: "Which storage engine is current?".to_string(),
        budget: Some(MemoryBudget {
            tokens: 30_000,
            detail: MemoryDetailLevel::Balanced as i32,
            depth: 3,
            max_entries: 0,
            max_bytes: PAGE_BYTES,
        }),
        page: None::<PageRequest>,
        ..Default::default()
    };
    let projected = project_ask_response(response, &request).expect("projects");
    let typed = projected.projection.as_ref().expect("projection");
    assert!(typed.more_on_request > 0);
    assert!(typed.next_call.is_none());
    let value = ask_value(&projected);
    assert_eq!(
        value["projection"]["more_on_request"].as_u64(),
        Some(typed.more_on_request)
    );
}

#[test]
fn the_warning_fits_the_planning_envelope() {
    // The planning pass reserves the longest warning; this one must not be
    // it, or every projection's core would be sized against a new envelope.
    assert!(MORE_ON_REQUEST.len() < RESTART_WARNING.len());
}
