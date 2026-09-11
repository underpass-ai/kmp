#[path = "support/write_neighborhood_fixture.rs"]
mod fixture;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use serde_json::json;

#[tokio::test]
async fn local_rich_links_review_before_commit_even_without_strict_validation() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut proposed = packet();
    proposed["options"] = json!({"strict":false});
    let first = call(&server, "kmp_write_memory", proposed.clone()).await;
    assert_eq!(first["accepted"], false);
    assert_eq!(events(dir.path()).await, 0);
    assert!(first.get("receipt").is_none());
    assert!(first.get("clocks").is_none());
    assert!(
        first["neighborhood"]["items"]
            .as_array()
            .expect("neighborhood items")
            .iter()
            .all(|item| item["state"] == "proposed")
    );
    let committed = resume(&server, &first).await;
    assert_eq!(committed["status"], "committed", "{committed}");
    assert_eq!(events(dir.path()).await, 1);
    let replay = call(&server, "kmp_write_memory", proposed).await;
    assert_eq!(replay["status"], "replayed");
    assert_eq!(replay["receipt"], committed["receipt"]);
    assert_eq!(replay["clocks"], committed["clocks"]);
    assert_eq!(events(dir.path()).await, 1);
}

#[tokio::test]
async fn changed_neighborhood_refreshes_and_a_corrected_proposal_cannot_reuse_the_token() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let first = call(&server, "kmp_write_memory", packet()).await;
    seed(
        &server,
        "new-source",
        "observation",
        "R5 records an 82 MB copy, not 74 MB.",
        "copy",
    )
    .await;
    let refreshed = resume(&server, &first).await;
    assert_eq!(refreshed["status"], "needs_review");
    assert_ne!(
        first["neighborhood"]["token"],
        refreshed["neighborhood"]["token"]
    );
    assert_eq!(events(dir.path()).await, 1);
    let mut corrected = refreshed["next_actions"][0]["arguments"].clone();
    corrected["memories"][1]["summary"] = json!("R4 records 74 MB; R5 disagrees at 82 MB.");
    let correction = call(&server, "kmp_write_memory", corrected).await;
    assert_eq!(correction["status"], "needs_review");
    assert_ne!(
        correction["neighborhood"]["token"],
        refreshed["neighborhood"]["token"]
    );
    assert_eq!(events(dir.path()).await, 1);
    assert_eq!(resume(&server, &correction).await["status"], "committed");
}

#[tokio::test]
async fn irrelevant_scope_changes_reuse_review_and_simple_observations_need_no_review() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let first = call(&server, "kmp_write_memory", packet()).await;
    seed(
        &server,
        "unrelated",
        "observation",
        "The garden gate is open.",
        "garden",
    )
    .await;
    assert_eq!(resume(&server, &first).await["status"], "committed");
    assert_eq!(events(dir.path()).await, 2);
}

#[tokio::test]
async fn a_reviewed_preview_still_writes_nothing() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let mut request = packet();
    request["options"] = json!({"dry_run":true});
    let first = call(&server, "kmp_write_memory", request).await;
    let preview = resume(&server, &first).await;
    assert_eq!(preview["status"], "validated");
    assert_eq!(preview["accepted"], false);
    assert_eq!(events(dir.path()).await, 0);
}

#[tokio::test]
async fn retained_write_survives_restart_and_preserves_generated_identity() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let requests: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../plugins/kmp/guide/guide.requests.json"
    ))
    .expect("generated guide requests");
    for request in requests {
        call(&server, "kmp_ingest", request).await;
    }
    let before = events(dir.path()).await;
    let agent = call(
        &server,
        "kmp_guide",
        json!({"registration_key":"review-writer"}),
    )
    .await;
    let mut request = packet();
    request
        .as_object_mut()
        .expect("writer request")
        .remove("idempotency_key");
    request["context_id"] = agent["context_id"].clone();
    let first = call(&server, "kmp_write_memory", request).await;
    assert!(
        first["next_actions"][0]["arguments"]["continuation"].is_string(),
        "{first}"
    );
    drop(server);
    let server = KernelMcpServer::embedded(dir.path()).expect("restart");
    let committed = resume(&server, &first).await;
    assert_eq!(committed["status"], "committed");
    assert_eq!(committed["local_refs"], first["local_refs"]);
    assert_eq!(resume(&server, &first).await["status"], "replayed");
    assert_eq!(events(dir.path()).await, before + 1);
}
