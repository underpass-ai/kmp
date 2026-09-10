#[path = "support/write_neighborhood_fixture.rs"]
mod fixture;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use serde_json::json;

#[tokio::test]
async fn old_constraint_and_explicit_conflict_precede_recent_noise() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let limit = seed(
        &server,
        "old-limit",
        "constraint",
        "P9 limits the copy to 80 MB.",
        "copy",
    )
    .await;
    let older = seed(
        &server,
        "old-reading",
        "observation",
        "R4 records a 74 MB copy.",
        "copy",
    )
    .await;
    let disagreement = call(&server, "kmp_write_memory", json!({"about":ABOUT,"actor":"writer",
        "idempotency_key":"disagreement","labels":{"task":["copy"]},"memories":[{
            "id":"conflict","kind":"observation","summary":"R5 records 82 MB for the same copy; the readings disagree.",
            "evidence":"R5: 82 MB; R4: 74 MB.","connect_to":[{"ref":older,"rel":"contradicts","class":"evidential",
                "why":"R4 and R5 report different sizes for the same copy.","evidence":"R4: 74 MB; R5: 82 MB."}]}]})).await;
    assert_eq!(resume(&server, &disagreement).await["status"], "committed");
    for index in 0..8 {
        seed(
            &server,
            &format!("noise-{index}"),
            "observation",
            &format!("The copy worker emitted heartbeat {index}."),
            "copy",
        )
        .await;
    }
    seed(
        &server,
        "unrelated-limit",
        "constraint",
        "The garden gate must stay closed.",
        "garden",
    )
    .await;
    let mut proposed = packet();
    proposed["memories"]
        .as_array_mut()
        .expect("typed neighborhood fixture")
        .remove(0);
    proposed["memories"][0]["connect_to"][0]["ref"] = limit.clone();
    let review = call(&server, "kmp_write_memory", proposed).await;
    let items = review["neighborhood"]["items"]
        .as_array()
        .expect("typed neighborhood fixture");
    assert!(items.iter().any(|item| item["ref"] == limit), "{review}");
    assert_eq!(
        items
            .iter()
            .filter(|item| item["reason"] == "explicit_conflict")
            .count(),
        2,
        "{review}"
    );
    assert!(!items.iter().any(|item| {
        item["text"]
            .as_str()
            .is_some_and(|text| text.contains("garden"))
    }));
    assert_eq!(review["neighborhood"]["partial"], true);
    assert!(
        review["neighborhood"]["omitted"]
            .as_u64()
            .expect("typed neighborhood fixture")
            > 0
    );
    assert_eq!(review["neighborhood"]["omitted_conflicts"], 0);
    let links = review["neighborhood"]["links"]
        .as_array()
        .expect("typed neighborhood fixture");
    let conflict = links
        .iter()
        .find(|link| link["rel"] == "contradicts")
        .expect("conflict direction");
    assert_eq!(
        items[conflict["to"].as_u64().expect("typed neighborhood fixture") as usize]["ref"],
        older
    );
}

#[tokio::test]
async fn long_source_is_omitted_whole_and_exact_inspect_preserves_qualification() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let text = format!(
        "{} This does not authorize production use.",
        "The report describes an experimental copy. ".repeat(12)
    );
    let source = seed(&server, "long-source", "constraint", &text, "copy").await;
    let first = call(&server, "kmp_write_memory", packet()).await;
    let item = first["neighborhood"]["items"]
        .as_array()
        .expect("typed neighborhood fixture")
        .iter()
        .find(|item| item["ref"] == source)
        .expect("constraint in review");
    assert_eq!(item["text_omitted"], true);
    assert!(item["text"].is_null());
    assert_eq!(first["neighborhood"]["partial"], true);
    assert!(item["clocks"].get("occurred_at").is_none());
    assert!(item["clocks"]["observed_at"].is_array());
    let detail = call(&server, "kmp_inspect", item["action"]["arguments"].clone()).await;
    assert_eq!(detail["object"]["text"], text);
}
