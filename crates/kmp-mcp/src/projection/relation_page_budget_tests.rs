//! A Trace page whose next relation alone is larger than `budget.max_bytes`
//! returns that relation with its prose shortened and advances, never an
//! empty page.
use serde_json::{Value, json};

use super::relation_page_budget::RelationPageBudget;
use super::serialized_size::serialized_len;

fn relation(index: usize, why: &str) -> Value {
    json!({"from":format!("entry:a{index}"),"to":format!("entry:b{index}"),"rel":"causes",
        "class":"causal","why":why,"evidence":"Log 14.","confidence":"high"})
}

#[test]
fn a_relation_larger_than_the_allowance_is_returned_shortened() {
    let long = "The pump started because the reserve valve froze. ".repeat(200);
    let value = json!({"summary":"trace","trace":[relation(0, &long), relation(1, "short")],
        "page":{"offset":0,"returned":2,"total":2,"has_more":false,"next_cursor":null},
        "quality":{"nodes":4,"relationships":2},"warnings":[]});
    let arguments = json!({"about":"project:x","from":"entry:a0","budget":{"max_bytes":2048}});
    let page = RelationPageBudget::Trace
        .apply(value, &arguments, "fingerprint")
        .expect("page");
    assert!(serialized_len(&page) <= 2048, "{}", serialized_len(&page));
    assert_eq!(page["page"]["returned"], 1, "{page}");
    assert_eq!(page["trace"][0]["from"], "entry:a0");
    let why = page["trace"][0]["why"].as_str().expect("why");
    assert!(why.ends_with('…') && why.len() < long.len(), "{why}");
    assert!(
        page["page"]["required_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes > 2048)
    );
    assert_eq!(page["page"]["has_more"], true);
    assert!(
        page["warnings"]
            .as_array()
            .expect("warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .is_some_and(|text| text.contains("shortened")))
    );
}
