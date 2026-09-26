//! The echoed request is the request: a detail the caller left unnamed is
//! not named on the continuation, whose kernel query must equal the first
//! page's for the page to be cut from that read.

use kmp_proto::v1beta1::{AskRequest, MemoryBudget, MemoryDetailLevel, WakeRequest};

use super::request_arguments::{ask_arguments, wake_arguments};

fn budget(detail: MemoryDetailLevel) -> Option<MemoryBudget> {
    Some(MemoryBudget {
        detail: detail as i32,
        max_bytes: 4_096,
        ..MemoryBudget::default()
    })
}

#[test]
fn an_unnamed_detail_is_not_echoed_as_balanced() {
    let ask = AskRequest {
        about: "project:kmp".into(),
        question: "why?".into(),
        budget: budget(MemoryDetailLevel::Unspecified),
        ..AskRequest::default()
    };
    assert!(ask_arguments(&ask)["budget"].get("detail").is_none());
    let wake = WakeRequest {
        about: "project:kmp".into(),
        budget: None,
        ..WakeRequest::default()
    };
    assert!(wake_arguments(&wake)["budget"].get("detail").is_none());
}

#[test]
fn a_named_detail_is_echoed_as_named() {
    for (detail, label) in [
        (MemoryDetailLevel::Compact, "compact"),
        (MemoryDetailLevel::Balanced, "balanced"),
        (MemoryDetailLevel::Full, "full"),
    ] {
        let ask = AskRequest {
            about: "project:kmp".into(),
            question: "why?".into(),
            budget: budget(detail),
            ..AskRequest::default()
        };
        assert_eq!(ask_arguments(&ask)["budget"]["detail"], label);
        let wake = WakeRequest {
            about: "project:kmp".into(),
            budget: budget(detail),
            ..WakeRequest::default()
        };
        assert_eq!(wake_arguments(&wake)["budget"]["detail"], label);
    }
}
