//! Source preservation, receipts and declaration clocks for relation-only writes.

#[path = "support/relation_write_fixture.rs"]
pub mod fixture;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

#[tokio::test]
async fn a_declared_link_leaves_both_sources_exactly_as_they_were() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;
    let before = [
        inspected(&server, &alias).await,
        inspected(&server, &notice).await,
    ];

    let written = write(&server, junco_link(&alias, &notice)).await;
    assert_eq!(written["status"], "committed", "{written}");

    for (reference, before) in [(&alias, &before[0]), (&notice, &before[1])] {
        let after = inspected(&server, reference).await;
        assert_eq!(after["object"]["text"], before["object"]["text"], "text");
        assert_eq!(after["object"]["kind"], before["object"]["kind"], "kind");
        assert_eq!(
            after["object"]["metadata"], before["object"]["metadata"],
            "metadata"
        );
        assert_eq!(
            after["raw"][0]["coordinates"],
            before["raw"][0]["coordinates"]
        );
        assert_eq!(after["raw"], before["raw"], "no source revision is written");
        // The evidence the source already carried is still there, unchanged,
        // beside whatever this write added.
        for earlier in before["evidence"].as_array().expect("stored evidence") {
            assert!(
                after["evidence"]
                    .as_array()
                    .expect("evidence")
                    .iter()
                    .any(|item| item["id"] == earlier["id"] && item["text"] == earlier["text"]),
                "`{reference}` lost evidence {earlier}: {after}"
            );
        }
    }
}

#[tokio::test]
async fn the_receipt_separates_the_created_link_from_the_untouched_sources() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;

    let written = write(&server, junco_link(&alias, &notice)).await;
    let attachment = &written["attachment"];
    assert_eq!(attachment["created"]["relations"][0]["from"], alias);
    assert_eq!(attachment["created"]["relations"][0]["rel"], "supports");
    assert_eq!(attachment["created"]["relations"][0]["to"], notice);
    assert_eq!(attachment["created"]["relations"][0]["observed_at"], LATE);
    assert_eq!(
        attachment["created"]["evidence"][0],
        attachment["created"]["relations"][0]["evidence_ref"]
    );
    assert_eq!(
        attachment["unchanged_sources"],
        json!([alias.clone(), notice.clone()]),
        "{written}"
    );
    assert_eq!(written["coverage"]["memories"], 0);
    assert_eq!(written["coverage"]["relations"], 1);
    assert_eq!(written["generated_refs"], json!([]));
    assert!(written.get("replacement").is_none(), "{written}");
    assert!(
        written["summary"]
            .as_str()
            .expect("summary")
            .starts_with("Attached 1 relation"),
        "{written}"
    );
}

#[tokio::test]
async fn replacing_a_memory_at_a_supplied_ref_says_so_instead_of_reading_as_an_attachment() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, _) = sources(&server).await;

    let written = write(
        &server,
        json!({"about":ABOUT,"actor":"agent:sol","idempotency_key":"junco-j01-rewrite",
            "observed_at":LATER,"labels":{"task":["junco"]},
            "memories":[{"id":"alias","ref":alias,"kind":"observation",
                "summary":"J01 registers Nora as the operational alias of Leonor Alba (corrected).",
                "evidence":"Junco register, corrected entry J01."}]}),
    )
    .await;
    assert_eq!(written["status"], "committed", "{written}");
    let replaced = &written["replacement"]["memories"][0];
    assert_eq!(replaced["ref"], alias);
    assert_eq!(replaced["observed_at"], LATER);
    assert_eq!(
        replaced["evidence"][0],
        format!("evidence:{alias}:current"),
        "{written}"
    );
    assert!(written.get("attachment").is_none(), "{written}");
}

#[tokio::test]
async fn the_link_is_dated_when_it_was_asserted_and_not_when_its_sources_were() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;
    write(&server, junco_link(&alias, &notice)).await;

    // Before the declaration neither the link nor its evidence exists, and
    // the alias still reads exactly as it did on 1 September — which is what
    // the resubmitted-source route could not promise.
    let before = at(&server, "2026-09-08T10:59:59Z", &alias).await;
    assert_eq!(before["entries"][0]["text"], ALIAS, "{before}");
    assert!(!links(&before, &alias, &notice), "{before}");
    assert!(
        !cites(&before, "J03 names Nora as responsible"),
        "the link's evidence must not be readable before it was declared: {before}"
    );

    let after = at(&server, LATE, &alias).await;
    assert_eq!(
        after["entries"][0]["text"], ALIAS,
        "still the stored source"
    );
    assert!(links(&after, &alias, &notice), "{after}");
    assert!(cites(&after, "J03 names Nora as responsible"), "{after}");
}

#[tokio::test]
async fn a_second_link_to_the_same_pair_keeps_the_first_link_and_its_evidence() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;
    let first = write(&server, junco_link(&alias, &notice)).await;

    let second = write(
        &server,
        packet(
            &alias,
            &notice,
            "junco-link-v2",
            LATER,
            "contributes_to",
            "The alias register is one of the inputs to the responsibility assignment.",
            "J03 cites the alias register among its inputs.",
        ),
    )
    .await;
    assert_eq!(second["status"], "committed", "{second}");

    let first_evidence = first["attachment"]["created"]["evidence"][0].clone();
    let second_evidence = second["attachment"]["created"]["evidence"][0].clone();
    assert_ne!(
        first_evidence, second_evidence,
        "a later link must not take the earlier link's evidence id"
    );
    assert_eq!(
        second["attachment"]["created"]["relations"][0]["observed_at"],
        LATER
    );

    let stored = inspected(&server, &alias).await;
    for expected in [&first_evidence, &second_evidence] {
        let expected = expected.as_str().expect("evidence ref");
        assert!(
            stored["evidence"]
                .as_array()
                .expect("evidence")
                .iter()
                .any(|item| item["id"].as_str().is_some_and(|id| id.ends_with(expected))),
            "{expected} is missing from {stored}"
        );
    }
    assert_eq!(stored["object"]["text"], ALIAS, "still the stored source");
}

async fn at(server: &KernelMcpServer, cut: &str, reference: &str) -> Value {
    call(
        server,
        "kmp_goto",
        json!({"about":ABOUT,"axis":"observed","at":{"time":cut},"refs":[reference],
            "budget":{"max_bytes":200000},"include":{"evidence":true,"relations":true}}),
    )
    .await
}

fn links(result: &Value, from: &str, to: &str) -> bool {
    result["proof"]["matched_relations"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(result["proof"]["path"].as_array().into_iter().flatten())
        .any(|relation| relation["from"] == from && relation["to"] == to)
}

fn cites(result: &Value, fragment: &str) -> bool {
    result["proof"]["evidence"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|item| {
            item["text"]
                .as_str()
                .is_some_and(|text| text.contains(fragment))
        })
}
