//! Shared source records and public write helpers for relation-only behavior tests.

use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

pub const ABOUT: &str = "project:junco";
pub const EARLY: &str = "2026-09-01T09:00:00Z";
pub const LATE: &str = "2026-09-08T11:00:00Z";
pub const LATER: &str = "2026-09-10T08:00:00Z";
pub const ALIAS: &str = "J01 registers Nora as the operational alias of Leonor Alba.";
pub const NOTICE: &str = "J03 assigns responsibility for the shared store to Nora.";
pub const CORRECTED: &str = "J01 corrected: the operational alias Nora belongs to another person.";

pub fn scratch() -> tempfile::TempDir {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&root).expect("scratch root");
    tempfile::tempdir_in(root).expect("isolated store")
}

pub async fn structured(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    let wire = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    serde_json::from_str::<Value>(&wire).expect("JSON")["result"].clone()
}

pub async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let result = structured(server, tool, arguments).await;
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}

pub async fn refused(server: &KernelMcpServer, arguments: Value) -> Value {
    let result = structured(server, "kmp_write_memory", arguments).await;
    assert_eq!(result["isError"], true, "{result}");
    result["structuredContent"].clone()
}

pub async fn write(server: &KernelMcpServer, arguments: Value) -> Value {
    let result = call(server, "kmp_write_memory", arguments).await;
    if result["status"] != "needs_review" {
        return result;
    }
    let action = &result["next_actions"][0];
    call(
        server,
        action["tool"].as_str().expect("verb"),
        action["arguments"].clone(),
    )
    .await
}

pub async fn seed(server: &KernelMcpServer, key: &str, id: &str, text: &str, at: &str) -> String {
    let result = write(
        server,
        json!({"about":ABOUT,"actor":"agent:sol","idempotency_key":key,"observed_at":at,
            "labels":{"task":["junco"]},
            "memories":[{"id":id,"kind":"observation","summary":text,
                "evidence":format!("Junco register, entry {id}.")}]}),
    )
    .await;
    assert_eq!(result["status"], "committed", "{result}");
    result["local_refs"][id]
        .as_str()
        .expect("canonical ref")
        .to_owned()
}

pub async fn sources(server: &KernelMcpServer) -> (String, String) {
    let alias = seed(server, "junco-j01", "alias", ALIAS, EARLY).await;
    let notice = seed(server, "junco-j03", "notice", NOTICE, LATE).await;
    (alias, notice)
}

pub async fn inspected(server: &KernelMcpServer, reference: &str) -> Value {
    call(
        server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":reference,"budget":{"max_bytes":200000},
            "include":{"details":true,"raw":true,"incoming":true,"outgoing":true}}),
    )
    .await
}

pub fn link(from: &str, to: &str, rel: &str, why: &str, evidence: &str) -> Value {
    json!({"from":from,"to":to,"rel":rel,"why":why,"evidence":evidence})
}

pub fn packet(
    from: &str,
    to: &str,
    key: &str,
    at: &str,
    rel: &str,
    why: &str,
    proof: &str,
) -> Value {
    json!({"about":ABOUT,"actor":"agent:sol","idempotency_key":key,"observed_at":at,
        "read_context":{"inspected_refs":[from,to]},
        "relations":[link(from, to, rel, why, proof)]})
}

pub fn junco_link(from: &str, to: &str) -> Value {
    packet(
        from,
        to,
        "junco-link-v1",
        LATE,
        "supports",
        "The alias register is what identifies the person the notice makes responsible.",
        "J01 registers the alias Nora; J03 names Nora as responsible.",
    )
}

pub async fn correct_alias(server: &KernelMcpServer, alias: &str) {
    let updated = write(
        server,
        json!({
            "about":ABOUT,"actor":"agent:sol","idempotency_key":"review-source-correction",
            "observed_at":LATER,"labels":{"task":["junco"]},
            "memories":[{"id":"alias","ref":alias,"kind":"observation",
                "summary":CORRECTED,"evidence":"Corrected Junco register."}]
        }),
    )
    .await;
    assert_eq!(updated["status"], "committed");
}
