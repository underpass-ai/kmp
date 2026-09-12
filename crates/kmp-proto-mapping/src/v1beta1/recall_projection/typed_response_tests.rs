//! What a projected page must still carry when it is read back onto the
//! typed wake or ask response.

use std::collections::BTreeSet;

use kmp_proto::v1beta1::{
    AskRequest, DimensionSelection, MemoryDetailLevel, SupersededMemory, WakeRequest,
};

use super::request_arguments::{ask_arguments, wake_arguments};
use super::response_value::{ask_value, wake_value};
use super::test_support::{projected, typed_ask_fixture, typed_wake_fixture};
use super::typed_recall::{project_ask_response, project_wake_response};

#[test]
fn bounded_ask_roundtrip_retains_every_projected_field() {
    let mut response = typed_ask_fixture(24);
    for (index, evidence) in response
        .proof
        .as_mut()
        .expect("proof")
        .evidence
        .iter_mut()
        .enumerate()
    {
        evidence.text = format!(
            "Fact {index}: {}",
            "The team verified the refresh race in request logs. ".repeat(20)
        );
    }
    for (index, reason) in response.because.iter_mut().enumerate() {
        reason.evidence = format!(
            "Rationale {index}: {}",
            "The selected retry directly addresses the observed refresh race. ".repeat(20)
        );
    }
    response.answer = "The selected retry addresses the refresh race. ".repeat(30);
    for max_bytes in [3_000, 6_500, 8_000, 12_000] {
        let mut request = AskRequest {
            about: "project:kmp".into(),
            question: "Which storage engine is current?".into(),
            budget: Some(kmp_proto::v1beta1::MemoryBudget {
                max_bytes,
                detail: MemoryDetailLevel::Full as i32,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut cursors = BTreeSet::new();
        loop {
            let expected = projected(ask_value(&response), ask_arguments(&request));
            let typed = project_ask_response(response.clone(), &request).expect("typed projection");
            let actual = ask_value(&typed);
            assert_eq!(
                actual, expected,
                "the typed round trip must retain the exact planned JSON at {max_bytes} bytes"
            );
            assert_eq!(
                actual["projection"]["budget"]["used_bytes"],
                serde_json::to_vec(&actual).expect("serialized page").len()
            );
            let page = typed.projection.expect("projection").page.expect("page");
            if !page.has_more {
                break;
            }
            let cursor = page.next_cursor.expect("continuation");
            if !cursors.insert(cursor.clone()) {
                assert!(
                    actual["projection"]["next_action"]["arguments"]["budget"]["max_bytes"]
                        .as_u64()
                        .is_some_and(
                            |bytes| bytes > request.budget.as_ref().expect("budget").max_bytes
                        ),
                    "a stalled page must explain how to continue"
                );
                let budget = request.budget.as_mut().expect("budget");
                assert!(
                    budget.max_bytes < 30_000,
                    "this fixture must advance with its full budget"
                );
                budget.max_bytes = 30_000;
            }
            assert!(cursors.len() <= 32, "finite fixture must finish");
            request.page = Some(kmp_proto::v1beta1::PageRequest { cursor, entries: 0 });
        }
    }
}

#[test]
fn shortened_typed_evidence_keeps_identity_text_and_budget() {
    let mut response = typed_ask_fixture(24);
    for evidence in &mut response.proof.as_mut().expect("proof").evidence {
        evidence.text = "No change to the 32 verified records. ".repeat(40);
        evidence.metadata.insert(
            "source_id".to_string(),
            "opaque-source-identity".to_string(),
        );
    }
    let mut request = AskRequest {
        about: "project:kmp".to_string(),
        question: "Which storage engine is current?".to_string(),
        budget: Some(kmp_proto::v1beta1::MemoryBudget {
            max_bytes: 8_000,
            detail: MemoryDetailLevel::Full as i32,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut seen = BTreeSet::new();
    for _ in 0..100 {
        let generic = projected(ask_value(&response), ask_arguments(&request));
        let result = project_ask_response(response.clone(), &request).expect("typed page");
        let actual = ask_value(&result);
        assert_eq!(actual["proof"]["evidence"], generic["proof"]["evidence"]);
        assert!(
            actual["projection"]["core_text_shortened"]
                .as_bool()
                .expect("shortened")
        );
        for reason in &result.because {
            let proof = result.proof.as_ref().expect("proof");
            let evidence = proof
                .evidence
                .iter()
                .find(|item| item.id == reason.r#ref)
                .expect("a cited evidence must never disappear when its body is shortened");
            assert_eq!(evidence.metadata["source_id"], "opaque-source-identity");
            assert!(!evidence.text.is_empty());
        }
        assert!(serde_json::to_vec(&actual).expect("bytes").len() <= 8_000);
        let page = result.projection.expect("projection").page.expect("page");
        if !page.has_more {
            return;
        }
        let cursor = page.next_cursor.expect("cursor");
        assert!(seen.insert(cursor.clone()), "cursor must advance");
        request.page = Some(kmp_proto::v1beta1::PageRequest { cursor, entries: 0 });
    }
    panic!("continuation did not terminate");
}

#[test]
fn typed_ask_projection_round_trips_exact_bytes_and_cursor() {
    let response = typed_ask_fixture(24);
    let mut request = AskRequest {
        about: "project:kmp".to_string(),
        question: "Which storage engine is current?".to_string(),
        budget: Some(kmp_proto::v1beta1::MemoryBudget {
            tokens: 30_000,
            detail: MemoryDetailLevel::Full as i32,
            depth: 3,
            max_entries: 0,
            max_bytes: 4_000,
        }),
        page: Some(kmp_proto::v1beta1::PageRequest {
            entries: 4,
            cursor: String::new(),
        }),
        ..Default::default()
    };

    let first = project_ask_response(response.clone(), &request).expect("first typed page");
    let first_value = ask_value(&first);
    let first_bytes = serde_json::to_vec(&first_value).expect("serialized typed projection");
    assert!(first_bytes.len() <= 4_000);
    assert_eq!(
        first
            .projection
            .as_ref()
            .and_then(|projection| projection.budget.as_ref())
            .expect("typed projection budget")
            .used_bytes,
        first_bytes.len() as u64
    );
    let cursor = first
        .projection
        .as_ref()
        .and_then(|projection| projection.page.as_ref())
        .and_then(|page| page.next_cursor.clone())
        .expect("typed continuation cursor");

    request.page.as_mut().expect("page").cursor = cursor;
    let second = project_ask_response(response.clone(), &request).expect("second typed page");
    let second_page = second
        .projection
        .as_ref()
        .and_then(|projection| projection.page.as_ref())
        .expect("second page accounting");
    assert_eq!(second_page.offset, 4);
    assert!(second_page.returned > 0);

    request.question = "A changed question".to_string();
    let error = project_ask_response(response, &request)
        .expect_err("a typed cursor is bound to the recall selection");
    assert!(error.to_string().contains("does not match"));
}

#[test]
fn typed_wake_projection_round_trips_exact_bytes_and_cursor() {
    let response = typed_wake_fixture(24);
    let mut request = WakeRequest {
        about: "project:kmp".to_string(),
        role: "implementer".to_string(),
        intent: "continue parity work".to_string(),
        budget: Some(kmp_proto::v1beta1::MemoryBudget {
            tokens: 30_000,
            detail: MemoryDetailLevel::Full as i32,
            depth: 3,
            max_entries: 0,
            max_bytes: 4_000,
        }),
        page: Some(kmp_proto::v1beta1::PageRequest {
            entries: 4,
            cursor: String::new(),
        }),
        ..Default::default()
    };

    let first = project_wake_response(response.clone(), &request).expect("first typed page");
    let first_value = wake_value(&first);
    let first_bytes = serde_json::to_vec(&first_value).expect("serialized typed projection");
    assert!(first_bytes.len() <= 4_000);
    assert_eq!(
        first
            .projection
            .as_ref()
            .and_then(|projection| projection.budget.as_ref())
            .expect("typed projection budget")
            .used_bytes,
        first_bytes.len() as u64
    );
    let cursor = first
        .projection
        .as_ref()
        .and_then(|projection| projection.page.as_ref())
        .and_then(|page| page.next_cursor.clone())
        .expect("typed continuation cursor");

    request.page.as_mut().expect("page").cursor = cursor;
    let second = project_wake_response(response, &request).expect("second typed page");
    let second_page = second
        .projection
        .as_ref()
        .and_then(|projection| projection.page.as_ref())
        .expect("second page accounting");
    assert_eq!(second_page.offset, 4);
    assert!(second_page.returned > 0);
}

#[test]
fn bounded_wake_roundtrip_preserves_long_causal_rationales() {
    let mut response = typed_wake_fixture(24);
    response.dimension_selection = Some(DimensionSelection::default());
    let wake = response.wake.as_mut().expect("wake");
    wake.objective = "Continue the verified storage rollout. ".repeat(80);
    wake.causal_spine[0].because =
        "The verified decision addresses the recorded storage requirement. ".repeat(80);
    for max_bytes in [3_000, 6_500, 8_000, 12_000] {
        let request = WakeRequest {
            about: "project:kmp".into(),
            budget: Some(kmp_proto::v1beta1::MemoryBudget {
                max_bytes,
                detail: MemoryDetailLevel::Full as i32,
                ..Default::default()
            }),
            ..Default::default()
        };
        let expected = projected(wake_value(&response), wake_arguments(&request));
        let actual =
            wake_value(&project_wake_response(response.clone(), &request).expect("typed wake"));
        assert_eq!(
            actual, expected,
            "wake must retain its planned rationale and proof"
        );
        assert_eq!(
            actual["projection"]["budget"]["used_bytes"],
            serde_json::to_vec(&actual).expect("bytes").len()
        );
        assert!(serde_json::to_vec(&actual).expect("bytes").len() <= max_bytes as usize);
    }
}

#[test]
fn shortened_wake_core_keeps_every_supersession_marker() {
    let mut response = typed_wake_fixture(24);
    response.proof.as_mut().expect("wake proof").superseded = (0..5)
        .map(|index| SupersededMemory {
            r#ref: format!("project:kmp:decision:old-{index}"),
            superseded_by: format!("project:kmp:decision:new-{index}"),
            why: format!(
                "The later decision {index} replaces the earlier one because {}",
                "the verified operating state changed ".repeat(24)
            ),
        })
        .collect();
    let request = WakeRequest {
        about: "project:kmp".to_string(),
        budget: Some(kmp_proto::v1beta1::MemoryBudget {
            tokens: 30_000,
            max_bytes: 4_000,
            detail: MemoryDetailLevel::Full as i32,
            depth: 1,
            max_entries: 0,
        }),
        ..Default::default()
    };

    let projected = project_wake_response(response, &request).expect("bounded wake");
    let proof = projected.proof.as_ref().expect("projected proof");
    assert_eq!(proof.superseded.len(), 5);
    for (index, marker) in proof.superseded.iter().enumerate() {
        assert_eq!(marker.r#ref, format!("project:kmp:decision:old-{index}"));
        assert_eq!(
            marker.superseded_by,
            format!("project:kmp:decision:new-{index}")
        );
        assert!(!marker.why.is_empty());
    }
    assert!(
        projected
            .projection
            .as_ref()
            .expect("projection")
            .core_text_shortened
    );
    assert!(
        serde_json::to_vec(&wake_value(&projected))
            .expect("serialized bounded wake")
            .len()
            <= 4_000
    );
}
