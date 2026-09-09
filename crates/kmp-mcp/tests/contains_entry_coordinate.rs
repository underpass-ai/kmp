//! Regression #576: redundant structural links retain entry coordinates.
use kmp_adapter_embedded::EmbeddedKernelStore;
use kmp_application::projection_mutations_for_context_event;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:dimensional-check";
const REF: &str = "project:dimensional-check:observation:registry";
const AT: &str = "2026-09-01T10:00:00Z";

async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{"name":name,"arguments":arguments}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    let response: Value = serde_json::from_str(&response).expect("JSON");
    assert!(response.get("error").is_none(), "{response}");
    assert_ne!(response["result"]["isError"], true, "{response}");
    response["result"]["structuredContent"].clone()
}

async fn inspect(server: &KernelMcpServer) -> Value {
    call(
        server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":REF,"include":{"raw":true}}),
    )
    .await
}

fn coordinates(inspected: &Value) -> &Vec<Value> {
    inspected["raw"]
        .as_array()
        .expect("raw records")
        .iter()
        .find(|record| record["ref"] == REF)
        .expect("record")["coordinates"]
        .as_array()
        .expect("coordinates")
}

#[tokio::test]
async fn explicit_contains_entry_preserves_clocks_and_rejects_unresolved_membership() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let mut args = json!({"about":ABOUT,"idempotency_key":"contains-entry:seed",
        "memory":{"dimensions":[{"id":"alias:neb","kind":"alias"},{"id":"component:neb","kind":"component"}],
            "entries":[{"id":REF,"kind":"observation","text":"The registry lists an alias and a component.",
                "coordinates":[{"dimension":"alias","scope_id":"alias:neb","occurred_at":AT,"observed_at":AT},{"dimension":"component","scope_id":"component:neb","occurred_at":AT,"observed_at":AT}]}],
            "relations":[{"from":"alias:neb","to":REF,"rel":"contains_entry","class":"structural","why":"The source explicitly assigns this alias to the entry."}]}});
    call(&server, "kmp_ingest", args.clone()).await;
    let before = inspect(&server).await;
    assert_eq!(coordinates(&before).len(), 2, "{before}");
    for coordinate in coordinates(&before) {
        assert_eq!(coordinate["observed_at"], AT);
        assert_eq!(coordinate["occurred_at"], AT);
        assert_eq!(coordinate["sequence"], 1);
        assert!(coordinate.get("ingested_at").is_some());
    }
    for (key, value) in [("alias", "alias:neb"), ("component", "component:neb")] {
        let selected = call(
            &server,
            "kmp_goto",
            json!({"about": ABOUT, "at": {"time": AT},
            "dimensions": {"selectors": [{"key": key, "op": "in", "values": [value]}]}}),
        )
        .await;
        assert_eq!(
            selected["entries"].as_array().map(Vec::len),
            Some(1),
            "{selected}"
        );
        assert_eq!(selected["entries"][0]["ref"], REF);
    }
    let store = EmbeddedKernelStore::open(directory.path()).expect("store");
    let bundle = store.export_bundle().await.expect("export");
    // A later write cannot erase the old membership when it has not supplied
    // an entry coordinate from which the structural link can be recovered.
    args["idempotency_key"] = json!("contains-entry:unresolved");
    args["memory"]["entries"][0]["id"] = json!(format!("{ABOUT}:observation:unrelated"));
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kmp_ingest","arguments":args}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    let response: Value = serde_json::from_str(&response).expect("JSON");
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert!(
        response
            .to_string()
            .contains("no matching entry membership")
    );
    let later = store.export_bundle().await.expect("export after refusal");
    assert_eq!(
        kmp_adapter_embedded::verify_bundle(&later)
            .expect("verified response")
            .content_digest,
        kmp_adapter_embedded::verify_bundle(&bundle)
            .expect("verified response")
            .content_digest
    );
    let target = tempfile::tempdir().expect("target");
    let restored = EmbeddedKernelStore::open(target.path()).expect("restore store");
    restored
        .import_bundle(&bundle, projection_mutations_for_context_event)
        .await
        .expect("import");
    drop(restored);
    let restored = KernelMcpServer::embedded(target.path()).expect("restored server");
    assert_eq!(inspect(&restored).await["raw"], before["raw"]);
}
