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
