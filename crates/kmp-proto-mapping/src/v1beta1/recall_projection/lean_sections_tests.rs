//! Lean progress: `projection.sections` reports only counters a host cannot
//! derive, and never a zero. The counts the older shape carried stay
//! recoverable: `eligible` = `core` + every page's `returned_on_page` +
//! the last `remaining`, and `total` = `eligible` + `excluded_by_detail`.

use std::collections::BTreeMap;

use kmp_proto::v1beta1::{MemoryBudget, MemoryDetailLevel, PageRequest, WakeRequest};
use serde_json::{Value, json};

use super::response_value::wake_value;
use super::test_support::{large_fixture, projected, typed_wake_fixture};
use super::typed_recall::project_wake_response;

const STATIC_KEYS: [&str; 2] = ["core", "excluded_by_detail"];
const PAGE_KEYS: [&str; 2] = ["returned_on_page", "remaining"];

fn counter(section: &Value, key: &str) -> u64 {
    section.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// Every counter present is a positive integer the shape allows; a page
/// that reuses the core restates none of the first page's static counts.
fn assert_lean(page: &Value) {
    let reused = page["projection"]["core_reused"] == true;
    let sections = page["projection"]["sections"]
        .as_object()
        .expect("sections object");
    for (name, section) in sections {
        let section = section.as_object().expect("section object");
        assert!(!section.is_empty(), "{name} is empty: {page}");
        for (key, value) in section {
            let allowed = PAGE_KEYS.contains(&key.as_str())
                || (!reused && STATIC_KEYS.contains(&key.as_str()));
            assert!(allowed, "{name}.{key} is derivable here: {page}");
            assert!(
                value.as_u64().is_some_and(|count| count > 0),
                "{name}.{key} carries a zero: {page}"
            );
        }
        // A counter's array on the page holds exactly what it reports.
        let path = format!("/{}", name.replace('.', "/"));
        let on_page = page
            .pointer(&path)
            .and_then(Value::as_array)
            .map_or(0, Vec::len) as u64;
        let core = if reused {
            0
        } else {
            counter(&json!(section), "core")
        };
        assert_eq!(
            on_page,
            core + counter(&json!(section), "returned_on_page"),
            "{name}: {page}"
        );
    }
}

/// Section → (eligible, excluded_by_detail), rebuilt from a page walk.
fn reconstruct(pages: &[Value]) -> BTreeMap<String, (u64, u64)> {
    let mut counts = BTreeMap::<String, (u64, u64)>::new();
    for (index, page) in pages.iter().enumerate() {
        let sections = page["projection"]["sections"]
            .as_object()
            .expect("sections");
        for (name, section) in sections {
            let entry = counts.entry(name.clone()).or_default();
            if index == 0 {
                entry.0 += counter(section, "core");
                entry.1 += counter(section, "excluded_by_detail");
            }
            entry.0 += counter(section, "returned_on_page");
            if index + 1 == pages.len() {
                assert_eq!(counter(section, "remaining"), 0, "{name}");
            }
        }
    }
    counts
}

fn json_arguments(detail: &str, max_bytes: u64) -> Value {
    json!({
        "about": "project:kmp",
        "question": "Which storage engine is current?",
        "budget": {"max_bytes": max_bytes, "detail": detail}
    })
}

#[test]
fn sections_omit_zero_and_derivable_counters_yet_reconstruct_every_count() {
    let packet = large_fixture(40);
    for detail in ["compact", "balanced", "full"] {
        let whole = projected(packet.clone(), json_arguments(detail, 1_000_000));
        assert_eq!(whole["projection"]["page"]["has_more"], false);
        assert_lean(&whole);
        let mut pages = vec![projected(packet.clone(), json_arguments(detail, 3_000))];
        while let Some(cursor) = pages
            .last()
            .and_then(|page| page.pointer("/projection/page/next_cursor"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
        {
            let mut arguments = json_arguments(detail, 3_000);
            arguments["page"] = json!({"cursor": cursor});
            pages.push(projected(packet.clone(), arguments));
            assert!(pages.len() < 60, "continuations must progress");
        }
        assert!(pages.len() > 1, "{detail}: the fixture must page");
        for page in &pages {
            assert_lean(page);
            assert!(page["projection"]["sections"].get("eligible").is_none());
        }
        let walked = reconstruct(&pages);
        assert_eq!(
            walked,
            reconstruct(std::slice::from_ref(&whole)),
            "{detail}"
        );
        let excluded: u64 = walked.values().map(|(_, excluded)| excluded).sum();
        assert_eq!(
            pages[0]["projection"]["excluded_by_detail"], excluded,
            "{detail}: per-section exclusions add up to the page total"
        );
    }
}

#[test]
fn typed_sections_round_trip_the_lean_shape() {
    let response = typed_wake_fixture(24);
    let request = |cursor: Option<String>| WakeRequest {
        about: "project:kmp".to_string(),
        budget: Some(MemoryBudget {
            tokens: 30_000,
            detail: MemoryDetailLevel::Balanced as i32,
            depth: 3,
            max_entries: 0,
            max_bytes: 3_000,
        }),
        page: cursor.map(|cursor| PageRequest {
            entries: 0,
            cursor,
            repeat_core: false,
        }),
        ..Default::default()
    };
    let mut pages = vec![wake_value(
        &project_wake_response(response.clone(), &request(None)).expect("first page"),
    )];
    while let Some(cursor) = pages
        .last()
        .and_then(|page| page.pointer("/projection/page/next_cursor"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
    {
        let page =
            project_wake_response(response.clone(), &request(Some(cursor))).expect("continuation");
        assert!(
            page.projection
                .as_ref()
                .expect("projection")
                .sections
                .iter()
                .all(|section| section.returned_on_page + section.remaining > 0),
            "a continuation lists only sections with delivery"
        );
        pages.push(wake_value(&page));
        assert!(pages.len() < 60, "continuations must progress");
    }
    assert!(pages.len() > 1, "the fixture must page");
    for page in &pages {
        assert_lean(page);
    }
}
