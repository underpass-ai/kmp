use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:writer-feedback";

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":arguments}});
    let reply = server
        .handle_json_line(&request.to_string())
        .await
        .expect("reply");
    let reply: Value = serde_json::from_str(&reply).expect("JSON reply");
    reply["result"].clone()
}

fn packet() -> Value {
    json!({
        "about":ABOUT,"actor":"writer","observed_at":"2026-09-01T09:00:00Z",
        "labels":{"component":["cache"]},
        "memories":[
            {"id":"logs","kind":"observation","summary":"The cache logs show failed requests after token refresh.","evidence":"R1 shows failed requests after token refresh and lists the names Cache Blue and CB.","labels":{"alias":["Cache Blue","CB"]}},
            {"id":"decision","kind":"decision","summary":"The team adopts cache retry.","evidence":"Decision D1 chooses a retry after reading R1.","connect_to":[{"ref":"@logs","rel":"chosen_because","class":"causal","why":"The observed failures led the team to choose a retry.","evidence":"D1 explicitly cites the R1 failures."}]}
        ]
    })
}

#[tokio::test]
async fn refusals_locate_the_member_and_rule_without_writing_any_member() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("store reader");
    for (pointer, value, code, field) in [
        (
            "/memories/1/evidence",
            json!(""),
            "MEMORY_EVIDENCE_REQUIRED",
            "memories[1].evidence",
        ),
        (
            "/memories/1/connect_to/0/evidence",
            json!(""),
            "RELATION_PROOF_REQUIRED",
            "memories[1].connect_to[0].evidence",
        ),
        (
            "/memories/1/connect_to/0/ref",
            json!("@missing"),
            "UNKNOWN_LOCAL_REF",
            "memories[1].connect_to[0].ref",
        ),
        (
            "/memories/1/connect_to/0/class",
            json!("constraint"),
            "RELATION_CLASS_MISMATCH",
            "memories[1].connect_to[0].class",
        ),
        (
            "/memories/1/summary",
            json!("El despliegue de v0.7.0 se retrasó porque los auditores no firmaron."),
            "SEARCH_SUMMARY_REQUIRED",
            "memories[1].summary_en",
        ),
        (
            "/memories/1/id",
            json!("logs"),
            "DUPLICATE_LOCAL_ID",
            "memories[1].id",
        ),
    ] {
        let mut args = packet();
        *args.pointer_mut(pointer).expect("field") = value;
        let result = call(&server, "kmp_write_memory", args).await;
        assert_eq!(result["isError"], true, "{result}");
        let feedback = &result["structuredContent"]["feedback"][0];
        assert_eq!(feedback["code"], code, "{result}");
        assert_eq!(feedback["field"], field, "{result}");
        assert_eq!(feedback["severity"], "error");
        assert!(
            feedback["action"].is_null(),
            "do not invent evidence or a repair"
        );
        assert_eq!(
            verify_bundle(&store.export_bundle().await.expect("export"))
                .expect("bundle")
                .event_count,
            0
        );
    }
    let mut unknown = packet();
    unknown["memories"][1]["aliases"] = json!(["invented field"]);
    let result = call(&server, "kmp_write_memory", unknown).await;
    assert_eq!(
        result["structuredContent"]["feedback"][0]["field"],
        "memories[1].aliases"
    );
    assert_eq!(
        result["structuredContent"]["feedback"][0]["code"],
        "UNKNOWN_ARGUMENT"
    );
    let accepted = call(&server, "kmp_write_memory", packet()).await;
    assert_eq!(
        accepted["structuredContent"]["accepted"], true,
        "{accepted}"
    );
}

#[tokio::test]
async fn prior_context_feedback_can_be_executed_then_the_write_repaired() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let original = call(&server, "kmp_write_memory", packet()).await;
    let target = &original["structuredContent"]["local_refs"]["logs"];
    assert!(target.is_string(), "{original}");
    let mut followup = packet();
    followup["memories"] = json!([followup["memories"][1]]);
    followup["memories"][0]["connect_to"][0]["ref"] = target.clone();
    let rejected = call(&server, "kmp_write_memory", followup.clone()).await;
    let feedback = &rejected["structuredContent"]["feedback"][0];
    assert_eq!(feedback["code"], "PRIOR_CONTEXT_REQUIRED", "{rejected}");
    assert_eq!(feedback["field"], "memories[0].connect_to[0].ref");
    let action = &feedback["action"];
    assert_eq!(action["tool"], "kmp_inspect");
    assert_eq!(action["arguments"], json!({"about":ABOUT,"ref":target}));
    let inspected = call(
        &server,
        action["tool"].as_str().expect("tool"),
        action["arguments"].clone(),
    )
    .await;
    assert_eq!(inspected["isError"], false, "{inspected}");
    followup["read_context"] =
        json!({"inspected_refs":[inspected["structuredContent"]["object"]["ref"]]});
    let written = call(&server, "kmp_write_memory", followup).await;
    assert_eq!(written["structuredContent"]["accepted"], true, "{written}");
}
