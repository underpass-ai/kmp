use kmp_domain::{NodeDetailProjection, NodeProjection, ProjectionMutation, ProjectionWriter};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn run(store: &std::path::Path, verb: &str, input: &Value) -> std::process::Output {
    run_raw(store, verb, input.to_string().as_bytes())
}

fn run_raw(store: &std::path::Path, verb: &str, input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"))
        .args(["consolidation", verb])
        .env("KMP_MCP_DATA_DIR", store)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input)
        .expect("input");
    child.wait_with_output().expect("output")
}
fn successful(store: &std::path::Path, verb: &str, input: &Value) -> Value {
    let result = run(store, verb, input);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).expect("json")
}

#[tokio::test]
async fn capture_write_project_expand_and_stale_fallback_use_real_cli() {
    let dir = tempfile::tempdir().expect("scratch");
    let store = kmp_embedded::EmbeddedKernelStore::open(dir.path()).expect("store");
    let about = "project:cli-consolidation";
    let reference = "project:cli-consolidation:source";
    let text = "The release permit applies only to staging. ".repeat(60);
    store
        .apply_mutations(vec![
            ProjectionMutation::UpsertNode(NodeProjection {
                node_id: reference.into(),
                node_kind: "observation".into(),
                title: "Permit".into(),
                summary: "Permit".into(),
                status: "ACTIVE".into(),
                labels: vec!["entry".into()],
                properties: [
                    ("memory_about".into(), about.into()),
                    ("payload_text".into(), text.clone()),
                    (
                        "memory_payload_json".into(),
                        json!({"text":text}).to_string(),
                    ),
                    ("custom_metadata".into(), "retain me".into()),
                ]
                .into(),
                provenance: None,
            }),
            ProjectionMutation::UpsertNodeDetail(NodeDetailProjection {
                node_id: reference.into(),
                detail: text.clone(),
                revision: 1,
                content_hash: "public".into(),
            }),
        ])
        .await
        .expect("seed");
    let sources = successful(
        dir.path(),
        "sources",
        &json!({"about":about,"refs":[reference]}),
    );
    assert_eq!(sources[0]["body"], text);
    assert_eq!(sources[0]["properties"]["custom_metadata"], "retain me");
    assert!(sources[0]["properties"].get("payload_text").is_none());
    assert!(
        sources[0]["properties"]
            .get("memory_payload_json")
            .is_none()
    );
    let write = json!({"about":about,"view":"permits","expect_revision":0,"idempotency_key":"first","author":"source-reader","sources":{reference:sources[0]["stamp"]},"assertions":[{"source_ref":reference,"quote":"The release permit applies only to staging.","why":"The source expressly restricts the permit to staging.","claim":{"referent":"release:r7","predicate":"permit","value":"staging","temporal_scope":"release:r7","polarity":"affirmed","epistemic_status":"reported","qualifiers":["only staging"]}}]});
    let first = successful(dir.path(), "write", &write);
    assert_eq!(first, successful(dir.path(), "write", &write));
    let projected = successful(
        dir.path(),
        "project",
        &json!({"about":about,"view":"permits","max_bytes":2048}),
    );
    assert_eq!(projected["status"], "current");
    assert_eq!(projected["claims"][0]["source_refs"][0], reference);
    let expanded = successful(dir.path(), "read", &projected["expansion"]);
    assert_eq!(expanded["status"], "historical_audit");
    assert_eq!(first["status"], "accepted");
    assert_eq!(first["about"], about);
    assert_eq!(first["view"], "permits");
    assert_eq!(first["source_count"], 1);
    assert_eq!(first["claim_count"], 1);
    assert!(first.get("claims").is_none());
    assert!(first.get("sources").is_none());
    for field in ["about", "view", "revision", "authored_at", "author"] {
        assert_eq!(first[field], expanded["view"][field]);
    }
    assert_eq!(first["expansion"], projected["expansion"]);
    assert_eq!(
        successful(dir.path(), "read", &first["expansion"]),
        expanded
    );
    assert_eq!(
        expanded["view"]["sources"][0]["properties"]["payload_text"],
        text
    );
    assert_eq!(
        expanded["view"]["sources"][0]["properties"]["memory_payload_json"],
        json!({"text":text}).to_string()
    );
    assert_eq!(expanded["view"]["sources"][0]["stamp"], sources[0]["stamp"]);
    assert!(first.to_string().len() < 1024);
    assert!(projected.to_string().len() < expanded.to_string().len());
    store
        .apply_mutations(vec![ProjectionMutation::UpsertNodeDetail(
            NodeDetailProjection {
                node_id: reference.into(),
                detail: "Permit revoked".into(),
                revision: 1,
                content_hash: "public".into(),
            },
        )])
        .await
        .expect("mutate");
    let stale = successful(
        dir.path(),
        "project",
        &json!({"about":about,"view":"permits"}),
    );
    assert_eq!(stale["status"], "stale");
    assert!(stale["claims"].as_array().expect("claims").is_empty());
    assert_eq!(first, successful(dir.path(), "write", &write));
    assert_eq!(
        expanded,
        successful(dir.path(), "read", &first["expansion"])
    );
}

#[test]
fn malformed_request_does_not_create_a_store_and_unsupported_time_mode_is_explicit() {
    let dir = tempfile::tempdir().expect("scratch");
    let path = dir.path().join("unopened");
    let result = run(
        &path,
        "read",
        &json!({"about":"project:x","view":"x","as_of":"2026-01-01T00:00:00Z"}),
    );
    assert!(!result.status.success());
    assert!(!path.exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unknown field"));
}

#[test]
fn invalid_requests_are_rejected_before_the_store_is_created() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut cases = vec![
        ("sources", json!({"about":" ","refs":["source"]})),
        (
            "sources",
            json!({"about":"x".repeat(513),"refs":["source"]}),
        ),
        ("sources", json!({"about":"project:x","refs":[]})),
        (
            "sources",
            json!({"about":"project:x","refs":["source","source"]}),
        ),
        ("sources", json!({"about":"project:x","refs":[" "]})),
        (
            "sources",
            json!({"about":"project:x","refs":(0..65).map(|n| format!("source:{n}")).collect::<Vec<_>>()}),
        ),
        ("read", json!({"about":"","view":"v"})),
        ("read", json!({"about":"project:x","view":"x".repeat(513)})),
        ("read", json!({"about":"project:x","view":"v","revision":0})),
        ("project", json!({"about":"project:x","view":" "})),
        (
            "project",
            json!({"about":"project:x","view":"v","max_bytes":511}),
        ),
        (
            "project",
            json!({"about":"project:x","view":"v","max_bytes":1048577}),
        ),
        (
            "project",
            json!({"about":"project:x","view":"missing","selection":{"axis":"occurred","as_of":"invalid"}}),
        ),
    ];
    let valid_write = json!({
        "about":"project:x", "view":"v", "expect_revision":0,
        "idempotency_key":"first", "author":"reader", "sources":{"source":format!("sha256:{}", "a".repeat(64))},
        "assertions":[{"source_ref":"source", "quote":"A passage", "why":"The passage supports the claim.",
            "claim":{"referent":"entity:x", "predicate":"has", "value":"permit", "temporal_scope":"event:x",
                "polarity":"affirmed", "epistemic_status":"reported", "qualifiers":[]}}]
    });
    for (pointer, invalid) in [
        ("/about", json!(" ")),
        ("/view", json!("")),
        ("/author", json!("x".repeat(513))),
        ("/idempotency_key", json!("")),
        ("/expect_revision", json!(u64::MAX)),
        ("/sources", json!({})),
        ("/sources/source", json!("not-a-stamp")),
        ("/assertions", json!([])),
        ("/assertions/0/source_ref", json!("undeclared")),
        ("/assertions/0/quote", json!(" ")),
        ("/assertions/0/why", json!("x".repeat(4097))),
        ("/assertions/0/claim/referent", json!("")),
        ("/assertions/0/claim/predicate", json!("x".repeat(4097))),
        ("/assertions/0/claim/value", json!(" ")),
        ("/assertions/0/claim/temporal_scope", json!("")),
        ("/assertions/0/claim/qualifiers", json!([""])),
        (
            "/assertions/0/claim/qualifiers",
            json!(vec!["qualifier"; 33]),
        ),
    ] {
        let mut request = valid_write.clone();
        *request.pointer_mut(pointer).expect("test field") = invalid;
        cases.push(("write", request));
    }
    let mut too_many = valid_write.clone();
    too_many["assertions"] = json!(vec![valid_write["assertions"][0].clone(); 257]);
    cases.push(("write", too_many));
    for (index, (verb, request)) in cases.into_iter().enumerate() {
        let path = dir.path().join(format!("unopened-{index}"));
        let result = run(&path, verb, &request);
        assert!(
            !result.status.success(),
            "case {index}: {verb} accepted invalid input"
        );
        assert!(
            !path.exists(),
            "case {index}: {verb} created a store before rejecting: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let path = dir.path().join("malformed-json");
    assert!(!run_raw(&path, "write", b"{invalid").status.success());
    assert!(!path.exists());
}

#[test]
fn invalid_cutoff_is_rejected_even_when_the_view_is_missing() {
    let dir = tempfile::tempdir().expect("scratch");
    let missing = successful(
        dir.path(),
        "project",
        &json!({"about":"project:x","view":"missing"}),
    );
    assert_eq!(missing["status"], "missing");
    let result = run(
        dir.path(),
        "project",
        &json!({
            "about":"project:x","view":"missing",
            "selection":{"axis":"occurred","as_of":"2026-99-01T00:00:00Z"}
        }),
    );
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("invalid consolidation as_of instant")
    );
}
