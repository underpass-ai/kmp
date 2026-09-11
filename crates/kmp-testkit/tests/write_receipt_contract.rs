//! #691: what a harness is allowed to treat as a write that landed.
//!
//! Every receipt here comes back from the real native writer, so the contract
//! is checked against what the kernel actually answers rather than a
//! hand-written shape.

use kmp_mcp::KernelMcpServer;
use kmp_testkit::WriteReceipt;
use serde_json::{Value, json};

const ABOUT: &str = "project:pending-writes";

async fn call(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
        "params":{"name":name,"arguments":arguments}})
    .to_string();
    let response = server.handle_json_line(&request).await.expect("response");
    let value: Value = serde_json::from_str(&response).expect("JSON");
    // The only check the consumers used to make.
    assert_ne!(
        value["result"]["isError"].as_bool(),
        Some(true),
        "{}",
        value["result"]
    );
    value["result"]["structuredContent"].clone()
}

fn seed_packet() -> Value {
    json!({"about":ABOUT,"actor":"writer","idempotency_key":"seed","labels":{"task":["copy"]},
        "memories":[{"id":"limit","kind":"constraint","summary":"P9 limits the copy to 80 MB.",
            "evidence":"P9: maximum 80 MB."}]})
}

fn relation_packet(limit: &Value) -> Value {
    json!({"about":ABOUT,"actor":"writer","idempotency_key":"relation","labels":{"task":["copy"]},
        "memories":[{"id":"run","kind":"observation","summary":"R4 records a 74 MB copy.",
            "evidence":"R4: 74 MB.",
            "connect_to":[{"ref":limit,"rel":"satisfies_constraint","class":"constraint",
                "why":"The recorded 74 MB is below P9's 80 MB maximum.",
                "evidence":"R4: 74 MB; P9: maximum 80 MB."}]}]})
}

#[tokio::test]
async fn a_committed_write_is_the_only_thing_that_reads_as_one() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");

    let committed = call(&server, 1, "kmp_write_memory", seed_packet()).await;
    let receipt = WriteReceipt::read("kmp_write_memory", &committed);
    assert!(receipt.is_accepted(), "{committed}");
    assert!(receipt.require_accepted().is_ok());

    // The same key again replays rather than writing twice, and a replay is
    // still a write that landed.
    let replayed = call(&server, 2, "kmp_write_memory", seed_packet()).await;
    assert_eq!(replayed["status"], "replayed", "{replayed}");
    assert!(WriteReceipt::read("kmp_write_memory", &replayed).is_accepted());
}

#[tokio::test]
async fn a_preview_is_not_a_write() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut packet = seed_packet();
    packet["options"] = json!({"dry_run": true});

    let preview = call(&server, 1, "kmp_write_memory", packet).await;

    assert_eq!(preview["status"], "validated", "{preview}");
    let receipt = WriteReceipt::read("kmp_write_memory", &preview);
    assert!(!receipt.is_accepted());
    let refusal = receipt
        .require_accepted()
        .expect_err("a preview wrote nothing");
    assert!(refusal.contains("preview"), "{refusal}");
}

/// The defect itself: `isError: false` with `accepted: false` and
/// `status: needs_review` is a proposal, and a harness that reads or scores on
/// it grades a store that never held the memory.
#[tokio::test]
async fn a_pending_proposal_is_refused_and_its_review_is_left_to_the_writer() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let limit =
        call(&server, 1, "kmp_write_memory", seed_packet()).await["local_refs"]["limit"].clone();

    let pending = call(&server, 2, "kmp_write_memory", relation_packet(&limit)).await;

    assert_eq!(pending["accepted"], false, "{pending}");
    assert_eq!(pending["status"], "needs_review", "{pending}");
    let receipt = WriteReceipt::read("kmp_write_memory", &pending);
    assert!(receipt.needs_review());
    assert!(!receipt.is_accepted());
    let refusal = receipt
        .require_accepted()
        .expect_err("a proposal pending review has written nothing");
    assert!(
        refusal.contains("pending review") && refusal.contains("isError: false` is not a receipt"),
        "the refusal has to say why the call looked fine: {refusal}"
    );
    // The context travels with the refusal so the writer can decide; reading it
    // is not accepting it.
    assert!(
        receipt.review_context().is_some(),
        "the pending write carries the neighborhood its review needs"
    );

    // Nothing landed, which is what the refusal was protecting the reader from.
    let asked = call(
        &server,
        3,
        "kmp_ask",
        json!({"about":ABOUT,"question":"Did R4 satisfy the P9 limit?","depth":3,
            "budget":{"tokens":2048,"detail":"balanced","max_entries":10}}),
    )
    .await;
    assert!(
        !asked.to_string().contains("74 MB"),
        "the proposal is not in the store: {asked}"
    );
}

/// And the other answer: once the review is resolved deliberately, through the
/// continuation the write returned, the same rule accepts it.
#[tokio::test]
async fn the_same_write_reads_as_landed_once_its_review_is_resolved() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let limit =
        call(&server, 1, "kmp_write_memory", seed_packet()).await["local_refs"]["limit"].clone();
    let pending = call(&server, 2, "kmp_write_memory", relation_packet(&limit)).await;

    let action = &pending["next_actions"][0];
    let resolved = call(
        &server,
        3,
        action["tool"]
            .as_str()
            .expect("the review returns its own verb"),
        action["arguments"].clone(),
    )
    .await;

    assert_eq!(resolved["status"], "committed", "{resolved}");
    assert!(WriteReceipt::read("kmp_write_memory", &resolved).is_accepted());
    let asked = call(
        &server,
        4,
        "kmp_ask",
        json!({"about":ABOUT,"question":"Did R4 satisfy the P9 limit?","depth":3,
            "budget":{"tokens":2048,"detail":"balanced","max_entries":10}}),
    )
    .await;
    assert!(
        asked.to_string().contains("74 MB"),
        "after the review the store holds what was proposed: {asked}"
    );
}

#[tokio::test]
async fn a_canonical_ingest_answers_with_its_own_receipt() {
    let dir = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let memory: Value = serde_json::from_str(
        r##"{"dimensions":[{"id":"work:main","kind":"work"}],
             "entries":[{"id":"project:control:e1","kind":"observation",
                "text":"The weekly meeting moved to ten in the morning.",
                "coordinates":[{"dimension":"work","scope_id":"work:main",
                    "occurred_at":"2026-08-01T09:00:00Z","sequence":1}]}]}"##,
    )
    .expect("memory");

    let ingested = call(
        &server,
        1,
        "kmp_ingest",
        json!({"about":"project:control","idempotency_key":"i1","memory":memory}),
    )
    .await;

    // The canonical ingest has no review gate; what it answers is whether the
    // write is readable back.
    assert!(ingested["status"].is_null(), "{ingested}");
    assert!(WriteReceipt::read("kmp_ingest", &ingested).is_accepted());
}

#[tokio::test]
async fn a_result_carrying_no_receipt_is_refused_rather_than_assumed() {
    let receipt = WriteReceipt::read(
        "kmp_write_memory",
        &json!({"summary": "something happened"}),
    );

    let refusal = receipt
        .require_accepted()
        .expect_err("a consumer cannot verify what the result never said");

    assert!(refusal.contains("no acceptance receipt"), "{refusal}");
}
