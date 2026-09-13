//! Cursor bindings for the stable, per-about audit core.

use serde_json::json;

#[path = "support/summaries_audit_fixture.rs"]
#[allow(dead_code)]
mod summaries_audit_fixture;

use summaries_audit_fixture::{ABOUT, audit, call, seeded};

const OTHER: &str = "project:cursor-other";

/// A cursor for filtered entries still promises the per-about counts that
/// accompanied its first page. Aggregate totals and the filtered entries can
/// be unchanged while those counts move, so the continuation must restart.
#[tokio::test]
async fn a_cursor_rejects_a_changed_per_about_core_outside_its_state_filter() {
    let (_store, server) = seeded().await;
    let other = call(
        &server,
        "kmp_ingest",
        json!({
            "about": OTHER,
            "idempotency_key": "audit:cursor-core:other-seed",
            "memory": {"dimensions": [{"id": "work:main", "kind": "work"}], "entries": [
                {
                    "id": format!("{OTHER}:decision:refused"),
                    "kind": "decision",
                    "text": "El registro de despliegue espera una revisión.",
                    "metadata": {"summary_en": "the record", "summary_en_by": "agent:older"},
                    "coordinates": [{
                        "dimension": "work", "scope_id": "work:main",
                        "occurred_at": "2026-05-07T10:00:00Z", "sequence": 1
                    }]
                },
                {
                    "id": format!("{OTHER}:decision:move"),
                    "kind": "decision",
                    "text": "El despliegue espera la aprobación del comité.",
                    "metadata": {
                        "summary_en": "The deployment is waiting for committee approval.",
                        "summary_en_by": "agent:older"
                    },
                    "coordinates": [{
                        "dimension": "work", "scope_id": "work:main",
                        "occurred_at": "2026-05-07T11:00:00Z", "sequence": 2
                    }]
                }
            ]}
        }),
    )
    .await;
    assert_ne!(other["isError"], true, "{other}");

    let first_arguments = json!({
        "dimensions": {"scope": "all_abouts"},
        "states": ["refused"],
        "page": {"entries": 1}
    });
    let first = audit(&server, first_arguments).await;
    assert_eq!(first["entries"].as_array().expect("entries").len(), 1);
    let cursor = first["page"]["next_cursor"]
        .as_str()
        .expect("two refused entries need a continuation")
        .to_string();

    // A's missing entry becomes stands, while B's stands entry becomes
    // missing. The aggregate totals and both refused entries stay unchanged,
    // but the per-about counts in the stable core do not.
    let revised_a = call(
        &server,
        "kmp_write_memory",
        json!({
            "about": ABOUT,
            "actor": "agent:cursor",
            "search_summaries": [{
                "ref": format!("{ABOUT}:decision:valkey"),
                "summary_en": "Valkey 7.2 was adopted for the shared store (ADR-018)."
            }]
        }),
    )
    .await;
    assert_eq!(
        revised_a["structuredContent"]["accepted"], true,
        "{revised_a}"
    );
    let revised_b = call(
        &server,
        "kmp_ingest",
        json!({
            "about": OTHER,
            "idempotency_key": "audit:cursor-core:other-missing",
            "memory": {"dimensions": [], "entries": [{
                "id": format!("{OTHER}:decision:move"),
                "kind": "decision",
                "text": "El despliegue espera la aprobación del comité.",
                "coordinates": [{
                    "dimension": "work", "scope_id": "work:main",
                    "occurred_at": "2026-05-07T11:00:00Z", "sequence": 2
                }]
            }]}
        }),
    )
    .await;
    assert_ne!(revised_b["isError"], true, "{revised_b}");

    let continued = call(
        &server,
        "kmp_summaries_audit",
        json!({
            "dimensions": {"scope": "all_abouts"},
            "states": ["refused"],
            "page": {"entries": 1, "cursor": cursor}
        }),
    )
    .await;
    assert_eq!(continued["isError"], true, "{continued}");
    assert!(
        continued.to_string().contains("read the audit again"),
        "{continued}"
    );
}
