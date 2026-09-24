//! Endpoint validation and stored-context review must finish before any attachment commits.
#[path = "support/bound_action.rs"]
mod bound_action;

#[path = "support/relation_write_fixture.rs"]
pub mod fixture;
use fixture::*;
use kmp_mcp::KernelMcpServer;
use serde_json::json;

#[tokio::test]
async fn the_three_shapes_are_exclusive_and_nothing_that_moves_a_source_may_accompany_a_link() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;

    let mut both = junco_link(&alias, &notice);
    both["memories"] = json!([{"id":"x","kind":"observation","summary":"y","evidence":"z"}]);
    let refusal = refused(&server, both).await;
    assert_eq!(
        refusal["feedback"][0]["code"], "WRITE_OPERATION_REQUIRED",
        "{refusal}"
    );

    for field in ["labels", "occurred_at", "valid_from", "valid_until", "rank"] {
        let mut moving = junco_link(&alias, &notice);
        moving[field] = if field == "labels" {
            json!({"task":["junco"]})
        } else if field == "rank" {
            json!(2)
        } else {
            json!(LATE)
        };
        let refusal = refused(&server, moving).await;
        assert_eq!(
            refusal["feedback"][0]["code"], "PRESERVED_FIELD",
            "`{field}` must not accompany relations: {refusal}"
        );
    }

    let mut missing = junco_link(&alias, &notice);
    missing["relations"][0]["to"] = json!(format!("{ABOUT}:entry:observation:not-there"));
    let refusal = refused(&server, missing).await;
    assert!(
        refusal["error"]["message"]
            .as_str()
            .expect("message")
            .contains("references unknown refs"),
        "{refusal}"
    );

    let mut foreign = junco_link(&alias, &notice);
    foreign["relations"][0]["to"] = json!("project:other:entry:observation:elsewhere");
    let refusal = refused(&server, foreign).await;
    assert_eq!(
        refusal["feedback"][0]["code"], "CROSS_ABOUT_RELATION",
        "{refusal}"
    );

    let mut proofless = junco_link(&alias, &notice);
    proofless["relations"][0]
        .as_object_mut()
        .expect("link")
        .remove("evidence");
    let refusal = refused(&server, proofless).await;
    assert_eq!(
        refusal["feedback"][0]["code"], "RELATION_PROOF_REQUIRED",
        "{refusal}"
    );
    assert_eq!(
        refusal["feedback"][0]["field"], "relations[0].evidence",
        "{refusal}"
    );
}

#[tokio::test]
async fn a_rich_link_is_reviewed_before_it_commits_in_this_shape_too() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;

    let pending = call(&server, "kmp_write_memory", junco_link(&alias, &notice)).await;
    assert_eq!(pending["status"], "needs_review", "{pending}");
    assert_eq!(pending["accepted"], false, "nothing is written yet");
    assert_eq!(pending["relations"][0]["from"], alias, "{pending}");
    let resume = &pending["next_actions"][0];
    assert_eq!(resume["tool"], "kmp_write_memory");
    assert_eq!(
        bound_action::bound_arguments(&server, resume)["review_token"],
        pending["neighborhood"]["token"],
        "the continuation carries the token it was served"
    );
    // The alias is untouched while the review is outstanding.
    assert_eq!(inspected(&server, &alias).await["object"]["text"], ALIAS);

    let committed = call(&server, "kmp_write_memory", resume["arguments"].clone()).await;
    assert_eq!(committed["status"], "committed", "{committed}");
}

#[tokio::test]
async fn relation_review_includes_stored_labels_and_refreshes_after_a_source_correction() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;
    let constraint = write(
        &server,
        json!({
            "about":ABOUT,"actor":"agent:sol","idempotency_key":"identity-constraint",
            "observed_at":EARLY,"labels":{"task":["junco"]},
            "memories":[{"id":"identity-check","kind":"constraint",
                "summary":"An alias must be checked against the current person register.",
                "evidence":"Identity procedure, paragraph 2."}]
        }),
    )
    .await;
    let review = call(&server, "kmp_write_memory", junco_link(&alias, &notice)).await;
    let items = review["neighborhood"]["items"]
        .as_array()
        .expect("neighborhood");
    assert!(
        items.iter().any(
            |item| item["ref"] == constraint["local_refs"]["identity-check"]
                && item["reason"] == "scoped_constraint"
        ),
        "{review}"
    );
    assert!(
        items.iter().all(|item| item["state"] == "stored"),
        "{review}"
    );

    correct_alias(&server, &alias).await;
    let source = inspected(&server, &alias).await;
    let refreshed = call(
        &server,
        "kmp_write_memory",
        review["next_actions"][0]["arguments"].clone(),
    )
    .await;
    assert_eq!(refreshed["status"], "needs_review", "{refreshed}");
    assert_ne!(
        refreshed["neighborhood"]["token"],
        review["neighborhood"]["token"]
    );
    let committed = call(
        &server,
        "kmp_write_memory",
        refreshed["next_actions"][0]["arguments"].clone(),
    )
    .await;
    assert_eq!(committed["status"], "committed", "{committed}");
    assert_eq!(inspected(&server, &alias).await["raw"], source["raw"]);
}

#[tokio::test]
async fn one_unknown_endpoint_refuses_every_link_and_its_evidence() {
    let dir = scratch();
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded");
    let (alias, notice) = sources(&server).await;
    let before = inspected(&server, &alias).await;
    let mut request = junco_link(&alias, &notice);
    request["relations"][0]["rel"] = json!("follows");
    let mut missing = request["relations"][0].clone();
    missing["to"] = json!(format!("{ABOUT}:entry:missing"));
    request["relations"]
        .as_array_mut()
        .expect("declared relations")
        .push(missing);
    let error = refused(&server, request).await;
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("unknown refs"),
        "{error}"
    );
    let after = inspected(&server, &alias).await;
    for field in ["raw", "links", "evidence"] {
        assert_eq!(after[field], before[field], "{field}");
    }
}
