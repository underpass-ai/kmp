//! Late relation evidence must not travel back into observed history.
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

#[path = "support/unbridged_server.rs"]
mod unbridged_server;

const ABOUT: &str = "test:late-conflict";
const LATE_PROOF: &str =
    "The signed ledger received September 5 proves Maya held the entire shift.";

async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let response = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":name,"arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("MCP response");
    let response: Value = serde_json::from_str(&response).expect("JSON response");
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(response["result"]["isError"], false, "{response}");
    let result = response["result"]["structuredContent"].clone();
    assert_ne!(
        result["projection"]["page"]["has_more"], true,
        "complete proof required"
    );
    result
}

async fn write(
    server: &KernelMcpServer,
    key: &str,
    text: &str,
    seen: &str,
    target: Option<(&str, &str)>,
) -> String {
    let mut args = json!({"about":ABOUT,"actor":"test-writer","intent":"record_observation",
        "idempotency_key":key,"scope":{"process":"duty-review"},
        "occurred_at":"2026-09-01T08:00:00Z","observed_at":seen,"source_kind":"human",
        "current":{"kind":"observation","summary":text,"evidence":text}});
    if let Some((target, relation)) = target {
        args["read_context"] = json!({"inspected_refs":[target]});
        args["connect_to"] = json!([{"ref":target,"rel":relation,"class":"evidential",
            "confidence":"high","why":"These sole-operator assignments concern the same shift.",
            "evidence":if relation == "supersedes" { LATE_PROOF } else { "One report names Maya alone, the other Zoe alone." }}]);
    }
    let written = call(server, "kmp_write_memory", args).await;
    assert_eq!(written["accepted"], true);
    let reference = written["generated_refs"][0]
        .as_str()
        .expect("written ref")
        .to_string();
    call(
        server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":reference,"budget":{"max_bytes":40000}}),
    )
    .await;
    reference
}

fn query(tool: &str, selection: Value, axis: &str) -> Value {
    let mut args = json!({"about":ABOUT,"axis":axis,"budget":{"max_bytes":120000,"detail":"full"}});
    if tool == "kmp_ask" {
        args["question"] = json!("Who was the sole operator for the shift?");
    }
    args.as_object_mut()
        .expect("expected response shape")
        .extend(
            selection
                .as_object()
                .expect("expected response shape")
                .clone(),
        );
    args
}

#[tokio::test]
async fn observed_proof_excludes_late_support_and_resolves_only_after_receipt() {
    let directory = tempfile::tempdir().expect("store");
    let server = unbridged_server::open(directory.path());
    let first = write(
        &server,
        "first",
        "Report one names Maya as the sole operator for the shift.",
        "2026-09-02T09:00:00Z",
        None,
    )
    .await;
    let second = write(
        &server,
        "second",
        "Report two names Zoe as the sole operator for the same shift.",
        "2026-09-03T09:00:00Z",
        Some((&first, "contradicts")),
    )
    .await;
    let selections = [
        json!({"as_of":{"time":"2026-09-04T00:00:00Z"}}),
        json!({"interval":{"start":"2026-09-02T00:00:00Z","end":"2026-09-05T09:00:00Z"}}),
    ];
    let mut before = Vec::new();
    for tool in ["kmp_wake", "kmp_ask"] {
        for selection in &selections {
            let response = call(&server, tool, query(tool, selection.clone(), "observed")).await;
            assert_eq!(
                response["proof"]["conflicts"]
                    .as_array()
                    .expect("expected response shape")
                    .len(),
                1
            );
            assert_eq!(response["proof"]["superseded"], json!([]));
            before.push(response["proof"]["evidence"].clone());
        }
    }
    let resolved = write(&server, "resolved", "The signed ledger confirms Maya as the sole operator for the shift and replaces report two.",
        "2026-09-05T09:00:00Z", Some((&second,"supersedes"))).await;
    let mut index = 0;
    for tool in ["kmp_wake", "kmp_ask"] {
        for selection in &selections {
            let response = call(&server, tool, query(tool, selection.clone(), "observed")).await;
            let proof = &response["proof"];
            assert!(
                !proof.to_string().contains(LATE_PROOF),
                "future relation evidence leaked: {proof}"
            );
            assert!(
                !proof.to_string().contains(&resolved),
                "future source leaked: {proof}"
            );
            assert_eq!(
                proof["conflicts"]
                    .as_array()
                    .expect("expected response shape")
                    .len(),
                1
            );
            assert_eq!(proof["superseded"], json!([]));
            assert_eq!(
                proof["evidence"], before[index],
                "historical evidence changed"
            );
            index += 1;
        }
        let response = call(
            &server,
            tool,
            query(
                tool,
                json!({"as_of":{"time":"2026-09-06T00:00:00Z"}}),
                "observed",
            ),
        )
        .await;
        assert_eq!(
            response["proof"]["conflicts"],
            json!([]),
            "resolved conflict still live"
        );
        assert_eq!(response["proof"]["superseded"][0]["ref"], second);
        assert_eq!(
            response["proof"]["superseded"][0]["superseded_by"],
            resolved
        );
        assert!(response["proof"].to_string().contains(LATE_PROOF));
        assert!(
            response["proof"]["path"]
                .as_array()
                .expect("expected response shape")
                .iter()
                .any(|edge| edge["rel"] == "contradicts"),
            "the historical contradiction remains auditable"
        );
        let event = call(
            &server,
            tool,
            query(
                tool,
                json!({"as_of":{"time":"2026-09-01T08:00:00Z"}}),
                "occurred",
            ),
        )
        .await;
        assert!(
            event["proof"].to_string().contains(&resolved),
            "late source still describes the earlier event"
        );
        assert!(event["proof"].to_string().contains(LATE_PROOF));
    }
    let report = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":second,"budget":{"max_bytes":40000}}),
    )
    .await;
    assert_eq!(
        report["object"]["text"],
        "Report two names Zoe as the sole operator for the same shift."
    );
    assert!(
        report["links"]["incoming"]
            .as_array()
            .expect("expected response shape")
            .iter()
            .any(|edge| edge["rel"] == "supersedes")
    );
}
