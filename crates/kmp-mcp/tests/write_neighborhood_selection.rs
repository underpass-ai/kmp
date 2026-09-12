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

/// #683: the writer reads the neighborhood. Clocks reach it as RFC 3339, the
/// spelling every other surface answers with, never as the kernel's internal
/// `unix:<offset>:<nanos>` key — which is not a Unix timestamp either, so an
/// agent that tries to read it literally lands over three thousand years away.
#[tokio::test]
async fn neighborhood_clocks_reach_the_writer_as_rfc3339() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let limit = seed_at(
        &server,
        "dated-limit",
        "constraint",
        "P9 limits the copy to 80 MB.",
        "copy",
        json!({"observed_at":"2026-09-10T12:00:00Z","occurred_at":"2026-09-10T12:00:00.250Z"}),
    )
    .await;
    let mut proposed = packet();
    proposed["memories"]
        .as_array_mut()
        .expect("typed neighborhood fixture")
        .remove(0);
    proposed["memories"][0]["connect_to"][0]["ref"] = limit.clone();
    // An explicit offset names the same instant as its UTC spelling.
    proposed["memories"][0]["observed_at"] = json!("2026-09-11T08:30:00.125+02:00");

    let review = call(&server, "kmp_write_memory", proposed).await;

    let items = review["neighborhood"]["items"]
        .as_array()
        .expect("typed neighborhood fixture");
    let stored = items
        .iter()
        .find(|item| item["ref"] == limit)
        .expect("the proposed endpoint is in its own neighborhood");
    assert_eq!(stored["state"], "stored");
    assert_eq!(
        stored["clocks"]["observed_at"],
        json!(["2026-09-10T12:00:00Z"])
    );
    assert_eq!(
        stored["clocks"]["occurred_at"],
        json!(["2026-09-10T12:00:00.25Z"]),
        "sub-second digits survive; only trailing zeros go"
    );
    assert!(
        stored["clocks"]["ingested_at"][0]
            .as_str()
            .expect("a stored fact carries the clock the kernel gave it")
            .ends_with('Z'),
        "the clock the kernel set is rendered like the ones the writer set: {stored}"
    );
    let proposed_item = items
        .iter()
        .find(|item| item["state"] == "proposed")
        .expect("the proposed fact appears beside the stored one");
    assert_eq!(
        proposed_item["clocks"]["observed_at"],
        json!(["2026-09-11T06:30:00.125Z"]),
        "one neighborhood speaks one clock spelling, so its instants compare"
    );
    let rendered = review["neighborhood"].to_string();
    assert!(
        !rendered.contains("unix:"),
        "no internal clock key reaches the writer: {rendered}"
    );
}

/// A neighbor without clocks says so by carrying none. Rendering never invents
/// an instant, and never drops a value it cannot read.
#[tokio::test]
async fn absent_clocks_stay_absent_and_unreadable_ones_stay_visible() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let limit = seed(
        &server,
        "undated-limit",
        "constraint",
        "P9 limits the copy to 80 MB.",
        "copy",
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
    let undated = items
        .iter()
        .find(|item| item["state"] == "proposed")
        .expect("the proposed fact appears in its own neighborhood");
    assert_eq!(
        undated["clocks"],
        json!({}),
        "a fact the writer gave no clock keeps none: {undated}"
    );
    // A stored fact does carry the observation clock the kernel defaulted for
    // it; every clock it carries is still readable.
    let stored = items
        .iter()
        .find(|item| item["ref"] == limit)
        .expect("the stored endpoint appears");
    for (axis, values) in stored["clocks"]
        .as_object()
        .expect("typed neighborhood fixture")
    {
        for value in values.as_array().expect("typed neighborhood fixture") {
            let value = value.as_str().expect("typed neighborhood fixture");
            assert!(
                value.ends_with('Z') && value.starts_with("20"),
                "{axis} reaches the writer as an instant it can read: {value}"
            );
        }
    }
}

/// Rendering is presentation. What the neighborhood shows must not move what
/// review asks for, what the continuation resolves, or what the store keeps.
#[tokio::test]
async fn readable_clocks_change_nothing_that_was_selected_or_stored() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("server");
    let limit = seed_at(
        &server,
        "dated-limit",
        "constraint",
        "P9 limits the copy to 80 MB.",
        "copy",
        json!({"observed_at":"2026-09-10T12:00:00Z","occurred_at":"2026-09-10T12:00:00.250Z"}),
    )
    .await;
    let mut proposed = packet();
    proposed["memories"]
        .as_array_mut()
        .expect("typed neighborhood fixture")
        .remove(0);
    proposed["memories"][0]["connect_to"][0]["ref"] = limit.clone();

    let review = call(&server, "kmp_write_memory", proposed.clone()).await;
    assert_eq!(review["status"], "needs_review");
    let first_token = review["neighborhood"]["token"].clone();

    // A repeat of the same packet against the same material fingerprints the
    // same: the review token follows the material, not the rendering.
    let repeated = call(&server, "kmp_write_memory", proposed).await;
    assert_eq!(
        repeated["neighborhood"]["token"], first_token,
        "an unchanged neighborhood keeps its fingerprint"
    );

    // The continuation the review handed back still applies.
    let committed = resume(&server, &review).await;
    assert_eq!(committed["status"], "committed", "{committed}");

    // The store kept the clocks it was given, in its own canonical spelling.
    let inspected = call(
        &server,
        "kmp_inspect",
        json!({"about":ABOUT,"ref":limit,"include":{"details":true,"raw":true}}),
    )
    .await;
    let stored = inspected.to_string();
    assert!(
        stored.contains("2026-09-10T12:00:00.250Z"),
        "the stored clock keeps the precision it was written with: {stored}"
    );
}
