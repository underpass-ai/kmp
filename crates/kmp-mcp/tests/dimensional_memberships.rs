//! The breaking label contract through native MCP and portable persistence.
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

fn write() -> Value {
    json!({
        "about": ABOUT,
        "actor": "fixture",
        "source_kind": "human",
        "labels": {
            "alias": ["neb", "Nébula Cache"],
            "component": ["neb"],
            "agentic_process": ["registry"]
        },
        "occurred_at": AT,
        "observed_at": AT,
        "idempotency_key": "dimensions:write",
        "options": {
            "strict": false
        },
        "memories": [
            {
                "id": "current",
                "ref": REF,
                "kind": "observation",
                "summary": "The registry lists neb and Nébula Cache as aliases of the neb component.",
                "evidence": "Registry R1 lists these exact names for the component."
            }
        ]
    })
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

async fn selected(server: &KernelMcpServer, selectors: Value) -> Value {
    call(server,"kmp_goto",json!({"about":ABOUT,"at":{"time":AT},"dimensions":{"selectors":selectors},"include":{"evidence":true,"relations":true}})).await
}

#[tokio::test]
async fn multiple_memberships_survive_selection_relabel_and_portable_replay() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let written = call(&server, "kmp_write_memory", write()).await;
    assert_eq!(written["accepted"], true, "{written}");
    assert_eq!(
        written["labels"]["created"].as_array().map(Vec::len),
        Some(4)
    );
    let before = inspect(&server).await;
    let labels = coordinates(&before);
    assert_eq!(labels.len(), 4, "{before}");
    let refs: std::collections::BTreeSet<_> = labels
        .iter()
        .map(|c| c["scope_id"].as_str().expect("ref"))
        .collect();
    assert_eq!(refs.len(), 4);
    assert!(refs.contains("label:v1:project%3Adimensional-check:alias:neb"));
    assert!(refs.contains("label:v1:project%3Adimensional-check:component:neb"));
    for predicates in [
        json!([{"key":"alias","op":"in","values":["neb","Nébula Cache"]}]),
        json!([{"key":"alias","op":"in","values":["neb"]},{"key":"component","op":"in","values":["neb"]}]),
        json!([{"key":"alias","op":"in","values":["neb"]},{"key":"alias","op":"in","values":["Nébula Cache"]}]),
    ] {
        let page = selected(&server, predicates).await;
        assert_eq!(page["entries"].as_array().map(Vec::len), Some(1), "{page}");
        assert_eq!(page["entries"][0]["ref"], REF);
    }
    let missing = selected(&server, json!([{"key":"owner","op":"in","values":["neb"]}])).await;
    assert_eq!(missing["entries"].as_array().map(Vec::len), Some(0));
    let relabel = json!({"about":ABOUT,"ref":REF,"actor":"fixture","observed_at":"2026-09-02T10:00:00Z","add":{"alias":["NC","Nebula service"]},"remove":{"alias":["neb"]},"why":"Registry R2 retires the short alias and lists two new names.","idempotency_key":"dimensions:relabel"});
    let changed = call(&server, "kmp_relabel", relabel.clone()).await;
    assert_eq!(changed["accepted"], true, "{changed}");
    assert_eq!(changed["labels"]["now"].as_array().map(Vec::len), Some(5));
    assert!(
        changed["labels"]["now"]
            .as_array()
            .expect("verified response")
            .contains(&json!({"key":"component","value":"neb"}))
    );
    let replay = call(&server, "kmp_relabel", relabel).await;
    assert_eq!(replay["accepted"], true, "{replay}");
    let after = inspect(&server).await;
    assert_eq!(coordinates(&after).len(), 5);
    for coordinate in coordinates(&after) {
        assert_eq!(coordinate["occurred_at"], AT, "{coordinate}");
        assert_eq!(coordinate["observed_at"], AT, "{coordinate}");
    }
    assert_eq!(
        selected(&server, json!([{"key":"alias","op":"in","values":["neb"]}])).await["entries"],
        json!([])
    );
    assert_eq!(
        selected(&server, json!([{"key":"alias","op":"in","values":["NC"]}])).await["entries"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    drop(server);
    let source = EmbeddedKernelStore::open(directory.path()).expect("source");
    let bundle = source.export_bundle().await.expect("export");
    let header = kmp_adapter_embedded::verify_bundle(&bundle).expect("verify");
    assert_eq!(header.bundle_format, 3);
    assert_eq!(header.event_format, 2);
    let target = tempfile::tempdir().expect("target");
    let restored = EmbeddedKernelStore::open(target.path()).expect("restored");
    restored
        .import_bundle(&bundle, projection_mutations_for_context_event)
        .await
        .expect("import");
    drop(restored);
    let server = KernelMcpServer::embedded(target.path()).expect("restored server");
    let roundtrip = inspect(&server).await;
    assert_eq!(roundtrip["raw"], after["raw"]);
    assert_eq!(roundtrip["object"], after["object"]);
}

#[tokio::test]
async fn label_values_that_look_like_refs_are_literal_on_write_and_relabel() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let literal = "label:v1:project%3Aother:alias:neb";
    let mut request = write();
    request["labels"] = json!({"alias":[literal]});
    call(&server, "kmp_write_memory", request).await;
    let change = json!({"about":ABOUT,"ref":REF,"actor":"fixture","observed_at":AT,
        "add":{"component":[literal]},"remove":{"alias":[literal]},
        "why":"The registry assigns the literal identifier to the component facet.",
        "idempotency_key":"dimensions:literal"});
    let applied = call(&server, "kmp_relabel", change.clone()).await;
    assert!(
        applied["labels"]["now"]
            .as_array()
            .expect("verified response")
            .contains(&json!({"key":"component","value":literal}))
    );
    assert_eq!(
        applied["labels"]["created"],
        json!([{"key":"component","value":literal}])
    );
    let replay = call(&server, "kmp_relabel", change).await;
    assert_eq!(replay["labels"]["added"], applied["labels"]["added"]);
    let record = inspect(&server).await;
    let stored = coordinates(&record)
        .iter()
        .find(|c| c["dimension"] == "component")
        .expect("component");
    let identity = kmp_domain::MemoryDimensionIdentity::parse(
        stored["scope_id"].as_str().expect("verified response"),
    )
    .expect("identity");
    assert_eq!(identity.about(), ABOUT);
    assert_eq!(identity.dimension_id(), literal);
}

#[tokio::test]
async fn canonical_ingest_rejects_ambiguous_members_without_partial_writes() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let mut arguments = json!({"about":ABOUT,"idempotency_key":"dimensions:canonical",
        "memory":{"dimensions":[{"id":"neb","kind":"alias"},{"id":"neb","kind":"component"}],
            "entries":[{"id":REF,"kind":"observation","text":"The registry lists two facets with the same value.",
                "coordinates":[{"dimension":"alias","scope_id":"neb","observed_at":AT},{"dimension":"component","scope_id":"neb","observed_at":AT}]}],
            "relations":[{"from":"neb","to":REF,"rel":"contains_entry","class":"structural"}]}});
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kmp_ingest","arguments":arguments}});
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    let response: Value = serde_json::from_str(&response).expect("JSON");
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert!(response.to_string().contains("ambiguous across keys"));
    let store = EmbeddedKernelStore::open(directory.path()).expect("store");
    assert_eq!(
        kmp_adapter_embedded::verify_bundle(&store.export_bundle().await.expect("export"))
            .expect("header")
            .event_count,
        0
    );
    arguments["memory"]["relations"][0]["from"] =
        json!("label:v1:project%3Adimensional-check:alias:neb");
    let accepted = call(&server, "kmp_ingest", arguments).await;
    assert_eq!(
        accepted["memory"]["read_after_write_ready"], true,
        "{accepted}"
    );
    assert_eq!(coordinates(&inspect(&server).await).len(), 2);
}
