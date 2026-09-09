//! Trace and Relate must honor their advertised structuredContent byte ceiling.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

#[path = "support/relation_read_fixture.rs"]
mod fixture;
use fixture::{ABOUT, SECTIONS, call, seed};

fn items(value: &Value, sections: &[&str]) -> Vec<Value> {
    sections
        .iter()
        .flat_map(|section| {
            value[*section]
                .as_array()
                .expect("valid native fixture")
                .clone()
        })
        .collect()
}

async fn check(tool: &str, query: Value, sections: &[&str]) {
    let directory = tempfile::tempdir().expect("valid native fixture");
    let server = KernelMcpServer::embedded(directory.path()).expect("valid native fixture");
    seed(&server).await;
    let mut arguments = query;
    arguments["budget"] = json!({"max_bytes":100_000});
    let full = call(&server, tool, arguments.clone()).await;
    let expected = items(&full, sections);
    assert!(!expected.is_empty());
    arguments["budget"]["max_bytes"] = json!(512);
    let floor = call(&server, tool, arguments.clone()).await;
    assert_eq!(
        floor["page"]["returned"], 0,
        "oversized items must remain whole: {floor}"
    );
    assert_eq!(floor["page"]["has_more"], true);
    assert!(
        !floor["warnings"]
            .as_array()
            .expect("valid native fixture")
            .is_empty()
    );
    let allowance = floor["page"]["required_bytes"]
        .as_u64()
        .expect("actionable retry budget");
    assert!(allowance > 512);
    assert_eq!(floor["next_actions"][0]["tool"], tool);
    arguments = floor["next_actions"][0]["arguments"].clone();
    assert_eq!(arguments["budget"]["max_bytes"], allowance);
    let mut actual = Vec::new();
    for _ in 0..(expected.len() * 2 + 2) {
        let page = call(&server, tool, arguments.clone()).await;
        let current_limit = arguments["budget"]["max_bytes"]
            .as_u64()
            .expect("valid native fixture");
        if page["page"]["returned"] == 0 {
            let required = page["page"]["required_bytes"]
                .as_u64()
                .expect("valid native fixture");
            assert!(
                required > current_limit,
                "retry must increase an insufficient allowance"
            );
            arguments = page["next_actions"][0]["arguments"].clone();
            continue;
        }
        assert!(
            serde_json::to_vec(&page)
                .expect("valid native fixture")
                .len()
                <= current_limit as usize,
            "{page}"
        );
        let returned = items(&page, sections);
        assert!(!returned.is_empty(), "retry must make progress: {page}");
        assert_eq!(page["page"]["returned"], returned.len());
        let returned_count = returned.len();
        actual.extend(returned);
        if page["quality"].is_object() {
            assert_eq!(page["quality"]["relationships"], returned_count);
            assert_eq!(page["quality"]["truncated"], page["page"]["has_more"]);
        }
        if page["page"]["has_more"] == false {
            break;
        }
        arguments = page["next_actions"][0]["arguments"].clone();
    }
    assert_eq!(
        actual, expected,
        "all source text, why and evidence must survive pagination"
    );
    assert_eq!(floor["page"]["total"], full["page"]["total"]);
}

#[tokio::test]
async fn trace_byte_budget_preserves_whole_relations_and_resumes() {
    check(
        "kmp_trace",
        json!({"about":ABOUT,"from":format!("{ABOUT}:observation:3"),
        "to":format!("{ABOUT}:observation:0")}),
        &["trace"],
    )
    .await;
}

#[tokio::test]
async fn relate_byte_budget_preserves_section_order_and_resumes() {
    check("kmp_relate", json!({"about":ABOUT}), SECTIONS).await;
}
