//! Native semantic completion must preserve proof and atomic rejection.
use kmp_adapter_embedded::{EmbeddedKernelStore, verify_bundle};
use kmp_domain::KnownMemoryRelationType;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

fn store_dir() -> tempfile::TempDir {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&scratch).expect("scratch");
    tempfile::tempdir_in(scratch).expect("store")
}

async fn call(server: &KernelMcpServer, tool: &str, args: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":args}});
    let reply = server
        .handle_json_line(&request.to_string())
        .await
        .expect("reply");
    serde_json::from_str::<Value>(&reply).expect("JSON")["result"].clone()
}

fn packet() -> Value {
    json!({"about":"project:input-completion","actor":"writer",
        "observed_at":"2026-09-01T10:00:00Z","idempotency_key":"completion",
        "labels":{"component":["gateway"]},"memories":[
        {"id":"deployment","kind":"observation","summary":"The gateway deployment succeeded.",
         "evidence":"The check records a successful gateway deployment.","connect_to":[
            {"ref":"check","rel":"verified_by","why":"The check verifies the deployed version.",
             "evidence":"The check records the expected deployed version."}]},
        {"id":"check","kind":"observation","summary":"The check confirms the expected gateway version.",
         "evidence":"The check returned the expected deployed version."}]})
}

#[tokio::test]
async fn complete_every_unambiguous_class_without_replacing_explicit_choices() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    for relation in KnownMemoryRelationType::writer_relation_types() {
        let spec = relation.writer_spec().expect("spec");
        let mut args = packet();
        args["options"] = json!({"dry_run":true});
        args["memories"][0]["connect_to"][0]["rel"] = json!(relation.as_str());
        let reply = call(&server, "kmp_write_memory", args.clone()).await;
        let result = &reply["structuredContent"];
        if spec.allowed_classes().len() == 1 {
            assert_eq!(result["status"], "validated", "{relation:?}: {reply}");
            let canonical = &result["ingest_preview"]["memory"]["relations"][0];
            assert_eq!(canonical["class"], spec.allowed_classes()[0].as_str());
            assert_eq!(canonical["to"], result["local_refs"]["check"]);
            assert_eq!(
                canonical["evidence"],
                args["memories"][0]["connect_to"][0]["evidence"]
            );
        } else {
            assert_eq!(result["status"], "rejected", "{relation:?}: {reply}");
            let feedback = &result["feedback"][0];
            assert_eq!(feedback["code"], "RELATION_CLASS_REQUIRED");
            assert_eq!(feedback["field"], "memories[0].connect_to[0].class");
            assert_eq!(
                feedback["allowed_values"],
                json!(
                    spec.allowed_classes()
                        .iter()
                        .map(|c| c.as_str())
                        .collect::<Vec<_>>()
                )
            );
        }
        for class in spec.allowed_classes() {
            args["memories"][0]["connect_to"][0]["class"] = json!(class.as_str());
            let reply = call(&server, "kmp_write_memory", args.clone()).await;
            assert_eq!(reply["structuredContent"]["status"], "validated", "{reply}");
            assert_eq!(
                reply["structuredContent"]["ingest_preview"]["memory"]["relations"][0]["class"],
                class.as_str()
            );
        }
    }
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("bundle"))
            .expect("verify")
            .event_count,
        0
    );
}

#[tokio::test]
async fn invalid_targets_classes_and_missing_proof_reject_the_whole_packet() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let store = EmbeddedKernelStore::open(dir.path()).expect("reader");
    for target in ["missing", "@missing"] {
        let mut args = packet();
        args["memories"][0]["connect_to"][0]["ref"] = json!(target);
        let reply = call(&server, "kmp_write_memory", args).await;
        assert_eq!(
            reply["structuredContent"]["feedback"][0]["code"], "UNKNOWN_LOCAL_REF",
            "{reply}"
        );
        assert_eq!(
            reply["structuredContent"]["feedback"][0]["field"],
            "memories[0].connect_to[0].ref"
        );
    }
    for (field, value, code) in [
        ("class", json!("constraint"), "RELATION_CLASS_MISMATCH"),
        ("class", json!(null), "INVALID_TYPE"),
        ("evidence", json!(""), "RELATION_PROOF_REQUIRED"),
        ("ref", json!("deployment"), "SELF_RELATION"),
        (
            "ref",
            json!("project:other:entry:observation:check"),
            "CROSS_ABOUT_RELATION",
        ),
    ] {
        let mut args = packet();
        args["memories"][0]["connect_to"][0][field] = value;
        let reply = call(&server, "kmp_write_memory", args).await;
        let feedback = &reply["structuredContent"]["feedback"][0];
        assert_eq!(feedback["code"], code, "{reply}");
        if code == "RELATION_CLASS_MISMATCH" {
            assert_eq!(feedback["allowed_values"], json!(["evidential"]));
        }
    }
    let mut missing = packet();
    missing
        .as_object_mut()
        .expect("packet")
        .remove("observed_at");
    let reply = call(&server, "kmp_write_memory", missing).await;
    assert_eq!(
        reply["structuredContent"]["feedback"][0]["code"],
        "REQUIRED_FIELD"
    );
    assert_eq!(
        reply["structuredContent"]["feedback"][0]["field"],
        "observed_at"
    );
    assert_eq!(
        verify_bundle(&store.export_bundle().await.expect("bundle"))
            .expect("verify")
            .event_count,
        0
    );
}

#[tokio::test]
async fn exact_local_targets_commit_and_replay_with_completed_class() {
    let dir = store_dir();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let ack = call(&server, "kmp_write_memory", packet()).await;
    assert_eq!(ack["structuredContent"]["status"], "committed", "{ack}");
    drop(server);
    let server = KernelMcpServer::embedded(dir.path()).expect("restart");
    let replay = call(&server, "kmp_write_memory", packet()).await;
    assert_eq!(
        replay["structuredContent"]["status"], "replayed",
        "{replay}"
    );
    assert_eq!(
        replay["structuredContent"]["relations"],
        ack["structuredContent"]["relations"]
    );
    let action = &ack["structuredContent"]["receipt"]["action"];
    let receipt = call(
        &server,
        action["tool"].as_str().expect("tool"),
        action["arguments"].clone(),
    )
    .await;
    let receipt: Value = serde_json::from_str(
        receipt["structuredContent"]["object"]["text"]
            .as_str()
            .expect("receipt"),
    )
    .expect("JSON");
    let canonical = &receipt["receipt"]["canonical_memory"];
    assert_eq!(canonical["relations"][0]["class"], "evidential");
    assert_eq!(
        canonical["relations"][0]["to"],
        ack["structuredContent"]["local_refs"]["check"]
    );
    assert_eq!(
        receipt["receipt"]["writer"]["relation_quality"][0]["class"],
        "evidential"
    );
    for entry in canonical["entries"].as_array().expect("entries") {
        for coordinate in entry["coordinates"].as_array().expect("coordinates") {
            assert_eq!(
                kmp_domain::temporal_instant_rfc3339(
                    coordinate["observed_at"].as_str().expect("observed")
                ),
                Some("2026-09-01T10:00:00Z".into())
            );
            for clock in ["occurred_at", "valid_from", "valid_until"] {
                assert!(coordinate[clock].is_null());
            }
        }
    }
}
