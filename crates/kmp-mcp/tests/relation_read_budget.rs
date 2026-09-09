//! Trace and Relate must honor their advertised structuredContent byte ceiling.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:relation-budget";
const SECTIONS: &[&str] = &["facts", "declared", "coordinate", "tensions", "proposed"];

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("valid native fixture");
    let response: Value = serde_json::from_str(&response).expect("valid native fixture");
    assert!(response.get("error").is_none(), "{response}");
    assert_ne!(response["result"]["isError"], true, "{response}");
    response["result"]["structuredContent"].clone()
}

async fn seed(server: &KernelMcpServer) {
    let entries: Vec<_> = (0..4).map(|i| json!({
        "id":format!("{ABOUT}:observation:{i}"), "kind":"observation",
        "text":format!("Source {i}: {}", "The authored log retains its complete evidence. ".repeat(30)),
        "coordinates":[{"dimension":"component","scope_id":"component:export",
            "observed_at":"2026-09-01T10:00:00Z"}]
    })).collect();
    let relations: Vec<_> = (1..4).map(|i| json!({
        "from":format!("{ABOUT}:observation:{i}"), "to":format!("{ABOUT}:observation:{}",i-1),
        "rel":"uses_background", "class":"evidential", "confidence":"high",
        "why":format!("Source {i} uses the preceding log as background. {}", "Retain this full rationale. ".repeat(30)),
        "evidence":format!("Fictional source {i} explicitly cites its predecessor. {}", "The original source text remains verbatim. ".repeat(30))
    })).collect();
    call(server,"kmp_ingest",json!({"about":ABOUT,"idempotency_key":"relation-budget:seed",
        "memory":{"dimensions":[{"id":"component:export","kind":"component"}],"entries":entries,"relations":relations}})).await;
}

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
    arguments["budget"]["max_bytes"] = json!(allowance);
    arguments["page"] = json!({"cursor":floor["page"]["next_cursor"]});
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
            arguments["budget"]["max_bytes"] = json!(required);
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
        arguments["page"]["cursor"] = page["page"]["next_cursor"].clone();
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
