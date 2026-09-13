//! Regression coverage for summaries-audit response budgeting and refreshes.

use serde_json::json;

#[path = "support/summaries_audit_fixture.rs"]
#[allow(dead_code)]
mod summaries_audit_fixture;

use summaries_audit_fixture::{ABOUT, audit, call, entry, seeded, state_of};

/// A default-size response must include every byte of its cursor and next
/// action when deciding how many whole entries fit.
#[tokio::test]
async fn pagination_counts_the_complete_structured_content() {
    let (_store, server) = seeded().await;
    let entries = (0..100)
        .map(|number| {
            let mut entry = entry(
                &format!("observation:budget-{number}"),
                "observation",
                "El equipo registró una decisión pendiente para la auditoría semanal.",
                None,
                (number % 10) + 1,
            );
            entry["coordinates"][0]["sequence"] = json!(number + 10);
            entry
        })
        .collect::<Vec<_>>();
    let added = call(
        &server,
        "kmp_ingest",
        json!({
            "about": ABOUT,
            "idempotency_key": "audit:budget:complete-content",
            "memory": {"dimensions": [], "entries": entries}
        }),
    )
    .await;
    assert_ne!(added["isError"], true, "{added}");

    let body = audit(&server, json!({"about": ABOUT})).await;
    assert!(body["page"]["has_more"].as_bool().expect("has_more"));
    assert!(body["page"].get("required_bytes").is_none(), "{body}");
    assert!(
        serde_json::to_vec(&body)
            .expect("structured content serializes")
            .len()
            <= 10_000,
        "{body}"
    );
}

/// A deliberate revalidation must clear stale even when it is written by the
/// same actor and carries the same English summary as the preceding revision.
#[tokio::test]
async fn an_explicit_same_text_summary_refresh_clears_stale() {
    let (_store, server) = seeded().await;
    let corrected = call(
        &server,
        "kmp_ingest",
        json!({
            "about": ABOUT,
            "idempotency_key": "audit:stale:punctuation",
            "memory": {"dimensions": [], "entries": [{
                "id": format!("{ABOUT}:decision:ventana"),
                "kind": "decision",
                "text": "La ventana de despliegue se movió al martes por la tarde (#469)!",
                "metadata": {
                    "summary_en": "The rollout window moved to Tuesday afternoon (#469).",
                    "summary_en_by": "agent:older"
                },
                "coordinates": [{
                    "dimension": "work", "scope_id": "work:main",
                    "occurred_at": "2026-05-06T13:00:00Z", "sequence": 4
                }]
            }]}
        }),
    )
    .await;
    assert_ne!(corrected["isError"], true, "{corrected}");
    assert_eq!(
        state_of(
            &audit(&server, json!({"about": ABOUT})).await,
            "decision:ventana"
        )["weaknesses"][0]["signal"],
        "stale"
    );

    let refreshed = call(
        &server,
        "kmp_write_memory",
        json!({
            "about": ABOUT,
            "actor": "agent:older",
            "search_summaries": [{
                "ref": format!("{ABOUT}:decision:ventana"),
                "summary_en": "The rollout window moved to Tuesday afternoon (#469)."
            }]
        }),
    )
    .await;
    assert_ne!(refreshed["isError"], true, "{refreshed}");
    let body = audit(&server, json!({"about": ABOUT})).await;
    let entry = state_of(&body, "decision:ventana");
    assert!(entry.get("weaknesses").is_none(), "{entry}");
}

/// An implicit idempotency key identifies a rendering of the source that was
/// read, not just the caller's actor/ref/English words. Otherwise a later
/// source correction replays the old refresh and leaves its copied marker
/// stale forever.
#[tokio::test]
async fn an_implicit_refresh_revalidates_a_later_source_with_the_same_words() {
    let (_store, server) = seeded().await;
    let source_before = "La ventana de despliegue se movió al martes por la tarde (#469).";
    let source_after = "La ventana de despliegue se movió al martes por la tarde (#469)!";
    let summary = "The rollout window moved to Tuesday afternoon (#469).";
    let refresh = json!({
        "about": ABOUT,
        "actor": "agent:older",
        "search_summaries": [{
            "ref": format!("{ABOUT}:decision:ventana"),
            "summary_en": summary
        }]
    });

    let first = call(&server, "kmp_write_memory", refresh.clone()).await;
    assert_eq!(first["structuredContent"]["accepted"], true, "{first}");
    let after_first = call(
        &server,
        "kmp_inspect",
        json!({"about": ABOUT, "ref": format!("{ABOUT}:decision:ventana"), "include": {"raw": true}}),
    )
    .await;
    let first_revision = after_first["structuredContent"]["raw"][0]["revision"].clone();
    let first_metadata = after_first["structuredContent"]["object"]["metadata"].clone();
    let immediate_retry = call(&server, "kmp_write_memory", refresh.clone()).await;
    assert_eq!(
        immediate_retry["structuredContent"]["accepted"], true,
        "{immediate_retry}"
    );
    let after_retry = call(
        &server,
        "kmp_inspect",
        json!({"about": ABOUT, "ref": format!("{ABOUT}:decision:ventana"), "include": {"raw": true}}),
    )
    .await;
    assert_eq!(
        after_retry["structuredContent"]["raw"][0]["revision"], first_revision,
        "an immediate default retry reuses its original write"
    );

    let corrected = call(
        &server,
        "kmp_ingest",
        json!({
            "about": ABOUT,
            "idempotency_key": "audit:implicit-summary:source-correction",
            "memory": {"dimensions": [], "entries": [{
                "id": format!("{ABOUT}:decision:ventana"),
                "kind": "decision",
                "text": source_after,
                "metadata": first_metadata,
                "coordinates": [{
                    "dimension": "work", "scope_id": "work:main",
                    "occurred_at": "2026-05-06T13:00:00Z", "sequence": 4
                }]
            }]}
        }),
    )
    .await;
    assert_ne!(corrected["isError"], true, "{corrected}");
    assert_eq!(
        state_of(
            &audit(&server, json!({"about": ABOUT})).await,
            "decision:ventana"
        )["weaknesses"][0]["signal"],
        "stale"
    );

    let second = call(&server, "kmp_write_memory", refresh).await;
    assert_eq!(second["structuredContent"]["accepted"], true, "{second}");
    let after_second = call(
        &server,
        "kmp_inspect",
        json!({"about": ABOUT, "ref": format!("{ABOUT}:decision:ventana"), "include": {"raw": true}}),
    )
    .await;
    let second_metadata = after_second["structuredContent"]["object"]["metadata"].clone();
    let second_revision = after_second["structuredContent"]["raw"][0]["revision"].clone();
    let returned = call(
        &server,
        "kmp_ingest",
        json!({
            "about": ABOUT,
            "idempotency_key": "audit:implicit-summary:source-return",
            "memory": {"dimensions": [], "entries": [{
                "id": format!("{ABOUT}:decision:ventana"),
                "kind": "decision",
                "text": source_before,
                "metadata": second_metadata,
                "coordinates": [{
                    "dimension": "work", "scope_id": "work:main",
                    "occurred_at": "2026-05-06T13:00:00Z", "sequence": 4
                }]
            }]}
        }),
    )
    .await;
    assert_ne!(returned["isError"], true, "{returned}");
    let third = call(
        &server,
        "kmp_write_memory",
        json!({
            "about": ABOUT,
            "actor": "agent:older",
            "search_summaries": [{
                "ref": format!("{ABOUT}:decision:ventana"),
                "summary_en": summary
            }]
        }),
    )
    .await;
    assert_eq!(third["structuredContent"]["accepted"], true, "{third}");
    let after_third = call(
        &server,
        "kmp_inspect",
        json!({"about": ABOUT, "ref": format!("{ABOUT}:decision:ventana"), "include": {"raw": true}}),
    )
    .await;
    assert_ne!(
        after_third["structuredContent"]["raw"][0]["revision"], second_revision,
        "a returned source is a new source revision, not the original retry"
    );
    let body = audit(&server, json!({"about": ABOUT})).await;
    let entry = state_of(&body, "decision:ventana");
    assert!(entry.get("weaknesses").is_none(), "{entry}");
}

/// An explicit idempotency key remains the caller's retry boundary. A source
/// correction does not silently turn that retry into a new validation write.
#[tokio::test]
async fn an_explicit_summary_key_does_not_change_with_the_source() {
    let (_store, server) = seeded().await;
    let summary = "The rollout window moved to Tuesday afternoon (#469).";
    let refresh = json!({
        "about": ABOUT,
        "actor": "agent:older",
        "idempotency_key": "audit:explicit-summary:retry",
        "search_summaries": [{
            "ref": format!("{ABOUT}:decision:ventana"),
            "summary_en": summary
        }]
    });
    let first = call(&server, "kmp_write_memory", refresh.clone()).await;
    assert_eq!(first["structuredContent"]["accepted"], true, "{first}");
    let inspected = call(
        &server,
        "kmp_inspect",
        json!({"about": ABOUT, "ref": format!("{ABOUT}:decision:ventana"), "include": {"raw": true}}),
    )
    .await;
    let metadata = inspected["structuredContent"]["object"]["metadata"].clone();
    let corrected = call(
        &server,
        "kmp_ingest",
        json!({
            "about": ABOUT,
            "idempotency_key": "audit:explicit-summary:source-correction",
            "memory": {"dimensions": [], "entries": [{
                "id": format!("{ABOUT}:decision:ventana"),
                "kind": "decision",
                "text": "La ventana de despliegue se movió al martes por la tarde (#469)!",
                "metadata": metadata,
                "coordinates": [{
                    "dimension": "work", "scope_id": "work:main",
                    "occurred_at": "2026-05-06T13:00:00Z", "sequence": 4
                }]
            }]}
        }),
    )
    .await;
    assert_ne!(corrected["isError"], true, "{corrected}");
    let before_retry = call(
        &server,
        "kmp_inspect",
        json!({"about": ABOUT, "ref": format!("{ABOUT}:decision:ventana"), "include": {"raw": true}}),
    )
    .await;
    let revision = before_retry["structuredContent"]["raw"][0]["revision"].clone();
    let replay = call(&server, "kmp_write_memory", refresh).await;
    assert_eq!(replay["isError"], true, "{replay}");
    assert!(replay.to_string().contains("idempotency key"), "{replay}");
    let after_retry = call(
        &server,
        "kmp_inspect",
        json!({"about": ABOUT, "ref": format!("{ABOUT}:decision:ventana"), "include": {"raw": true}}),
    )
    .await;
    assert_eq!(
        after_retry["structuredContent"]["raw"][0]["revision"],
        revision
    );
    let body = audit(&server, json!({"about": ABOUT})).await;
    assert_eq!(
        state_of(&body, "decision:ventana")["weaknesses"][0]["signal"],
        "stale"
    );
}

/// Several deliberate renderings of one unchanged source have distinct
/// implicit identities; retrying the later one must still reuse its own.
#[tokio::test]
async fn a_later_implicit_summary_refresh_remains_replayable() {
    let (_store, server) = seeded().await;
    let first = json!({
        "about": ABOUT,
        "actor": "agent:older",
        "search_summaries": [{
            "ref": format!("{ABOUT}:decision:ventana"),
            "summary_en": "The rollout window moved to Tuesday afternoon (#469)."
        }]
    });
    let later = json!({
        "about": ABOUT,
        "actor": "agent:older",
        "search_summaries": [{
            "ref": format!("{ABOUT}:decision:ventana"),
            "summary_en": "Tuesday afternoon is the rollout window after the move (#469)."
        }]
    });
    let first_result = call(&server, "kmp_write_memory", first).await;
    assert_eq!(
        first_result["structuredContent"]["accepted"], true,
        "{first_result}"
    );
    let later_result = call(&server, "kmp_write_memory", later.clone()).await;
    assert_eq!(
        later_result["structuredContent"]["accepted"], true,
        "{later_result}"
    );
    let before_retry = call(
        &server,
        "kmp_inspect",
        json!({"about": ABOUT, "ref": format!("{ABOUT}:decision:ventana"), "include": {"raw": true}}),
    )
    .await;
    let revision = before_retry["structuredContent"]["raw"][0]["revision"].clone();
    let retry = call(&server, "kmp_write_memory", later).await;
    assert_eq!(retry["structuredContent"]["accepted"], true, "{retry}");
    let after_retry = call(
        &server,
        "kmp_inspect",
        json!({"about": ABOUT, "ref": format!("{ABOUT}:decision:ventana"), "include": {"raw": true}}),
    )
    .await;
    assert_eq!(
        after_retry["structuredContent"]["raw"][0]["revision"],
        revision
    );
}
