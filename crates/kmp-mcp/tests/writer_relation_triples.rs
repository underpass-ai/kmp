//! The compact review must describe the actual graph, including forward refs.
#[path = "support/reviewed_writer.rs"]
mod reviewed_writer;
use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const ABOUT: &str = "project:relation-review";

fn store_dir() -> tempfile::TempDir {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&scratch).expect("scratch");
    tempfile::tempdir_in(scratch).expect("store")
}

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}})
    .to_string();
    let reply = server.handle_json_line(&line).await.expect("reply");
    let reply: Value = serde_json::from_str(&reply).expect("JSON");
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    reviewed_writer::review_authored_write(server, reply["result"]["structuredContent"].clone())
        .await
}

fn packet() -> Value {
    json!({
        "about":ABOUT,"actor":"writer","observed_at":"2026-09-01T10:00:00Z",
        "idempotency_key":"directions","labels":{"component":["gateway"]},
        "memories":[
            {"id":"approval","kind":"decision","summary":"Approve deploying and checking the gateway.",
             "evidence":"A permits deployment D and check H.","connect_to":[
                {"ref":"@deployment","rel":"authorizes","class":"motivational","why":"A permits D.","evidence":"A names D."},
                {"ref":"@check","rel":"authorizes","class":"motivational","why":"A permits H.","evidence":"A names H."}]},
            {"id":"deployment","kind":"observation","summary":"The gateway deployment succeeded.",
             "evidence":"H confirms the deployed version and successful health response.","connect_to":[
                {"ref":"@check","rel":"verified_by","class":"evidential","why":"H verifies the deployed version and health.","evidence":"H records the expected version and a successful health response."}]},
            {"id":"check","kind":"observation","summary":"The check confirms the deployed gateway version and healthy response.",
             "evidence":"H records the expected version and a successful health response."}
        ]
    })
}

async fn canonical(server: &KernelMcpServer, ack: &Value) -> Value {
    let action = &ack["receipt"]["action"];
    let detail = call(
        server,
        action["tool"].as_str().expect("tool"),
        action["arguments"].clone(),
    )
    .await;
    let receipt: Value =
        serde_json::from_str(detail["object"]["text"].as_str().expect("text")).expect("receipt");
    receipt["receipt"]["canonical_memory"].clone()
}

fn assert_canonical_triples(ack: &Value, memory: &Value) {
    let triples = ack["relations"].as_array().expect("triples");
    let stored = memory["relations"].as_array().expect("canonical relations");
    assert_eq!(triples.len(), stored.len());
    for (triple, relation) in triples.iter().zip(stored) {
        assert_eq!(triple["rel"], relation["rel"]);
        for endpoint in ["from", "to"] {
            let reference = triple[endpoint].as_str().expect("endpoint");
            let expanded = reference
                .strip_prefix('@')
                .map_or(&triple[endpoint], |id| &ack["local_refs"][id]);
            assert_eq!(expanded, &relation[endpoint]);
        }
    }
}

#[tokio::test]
async fn preview_commit_and_replay_keep_every_compiled_direction() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    let mut args = packet();
    args["options"] = json!({"dry_run":true});
    let preview = call(&server, "kmp_write_memory", args).await;
    assert_eq!(preview["status"], "validated");
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("bundle"))
            .expect("verify")
            .event_count,
        0
    );
    assert_eq!(
        preview["relations"],
        json!([
            {"from":"@approval","rel":"authorizes","to":"@deployment"},
            {"from":"@approval","rel":"authorizes","to":"@check"},
            {"from":"@deployment","rel":"verified_by","to":"@check"}
        ])
    );
    assert_canonical_triples(&preview, &preview["ingest_preview"]["memory"]);
    let ack = call(&server, "kmp_write_memory", packet()).await;
    assert_eq!(ack["status"], "committed");
    assert_eq!(ack["relations"], preview["relations"]);
    assert_canonical_triples(&ack, &canonical(&server, &ack).await);
    drop(server);
    let server = KernelMcpServer::embedded(dir.path()).expect("reopened");
    let replay = call(&server, "kmp_write_memory", packet()).await;
    assert_eq!(replay["status"], "replayed");
    assert_eq!(replay["relations"], ack["relations"]);
    assert_canonical_triples(&replay, &canonical(&server, &replay).await);
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("bundle"))
            .expect("verify")
            .event_count,
        1
    );
}

#[tokio::test]
async fn existing_targets_stay_canonical_and_unknown_validity_stays_absent() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut seed = packet();
    seed["memories"] = json!([{"id":"invoice","kind":"observation","summary":"Invoice J1 is 18 USD.","evidence":"G reports J1 at 18 USD; onset is unknown."}]);
    let seed = call(&server, "kmp_write_memory", seed).await;
    assert_eq!(seed["relations"], json!([]));
    let target = seed["local_refs"]["invoice"].clone();
    call(&server, "kmp_inspect", json!({"about":ABOUT,"ref":target})).await;
    let mut correction = packet();
    correction["idempotency_key"] = json!("correction");
    correction["read_context"] = json!({"inspected_refs":[target]});
    correction["memories"] = json!([{"id":"corrected","kind":"observation","summary":"Invoice J1 is corrected to 12 USD.","evidence":"C corrects J1 only; onset is unknown.","connect_to":[{"ref":target,"rel":"corrects","class":"evidential","why":"C corrects the amount of J1.","evidence":"C explicitly corrects J1 to 12 USD."}]}]);
    let ack = call(&server, "kmp_write_memory", correction).await;
    assert_eq!(
        ack["relations"],
        json!([{"from":"@corrected","rel":"corrects","to":target}])
    );
    let memory = canonical(&server, &ack).await;
    assert_canonical_triples(&ack, &memory);
    for coordinate in memory["entries"][0]["coordinates"]
        .as_array()
        .expect("coordinates")
    {
        assert!(coordinate["valid_from"].is_null());
        assert!(coordinate["valid_until"].is_null());
    }
}
