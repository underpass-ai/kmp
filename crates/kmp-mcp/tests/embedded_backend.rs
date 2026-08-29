//! E3 acceptance: the embedded backend serves KMP tools in-process and
//! memory survives across sessions (fresh-machine criterion analog).

use kmp_adapter_embedded::SqliteQualityTelemetryReader;
use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_domain::TokenEstimator;
use kmp_mcp::{EmbeddedKernelMcpBackend, KernelMcpServer};
use serde_json::{Value, json};

fn tool_call(id: u64, name: &str, arguments: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    })
    .to_string()
}

fn ingest_arguments() -> Value {
    json!({
        "about": "question:e3",
        "idempotency_key": "ingest:e3-accept",
        "memory": {
            "dimensions": [{"id": "conversation:s1", "kind": "conversation"}],
            "entries": [
                {
                    "id": "question:e3:claim:e3",
                    "kind": "claim",
                    "text": "Embedded backend accepted.",
                    "metadata": {
                        "window": "10:00-10:20",
                        "probe_digest": "sha256:e3"
                    },
                    "coordinates": [{
                        "dimension": "conversation",
                        "scope_id": "conversation:s1",
                        "occurred_at": "2026-07-22T10:00:00Z",
                        "sequence": 1
                    }]
                },
                {
                    "id": "question:e3:claim:e3-detail",
                    "kind": "claim",
                    "text": "The in-process backend serves KMP.",
                    "coordinates": [{
                        "dimension": "conversation",
                        "scope_id": "conversation:s1",
                        "occurred_at": "2026-07-22T10:00:01Z",
                        "sequence": 2
                    }]
                }
            ],
            "relations": [{
                "from": "question:e3:claim:e3",
                "to": "question:e3:claim:e3-detail",
                "rel": "supports",
                "class": "evidential",
                "why": "The accepted claim answers the checkpoint question.",
                "evidence": "E3 acceptance fixture.",
                "confidence": "high",
                "motivation": "Preserve the acceptance rationale.",
                "method": "Embedded conformance probe.",
                "decision_id": "question:e3:decision:e3",
                "caused_by_node_id": "question:e3:claim:e3",
                "coordinate": {
                    "dimension": "conversation",
                    "scope_id": "conversation:s1",
                    "valid_from": "2026-07-22T10:00:00Z",
                    "valid_until": "2026-07-22T10:20:00Z",
                    "sequence": 2
                }
            }],
            "evidence": [{
                "id": "evidence:question:e3:claim:e3:fixture",
                "supports": ["question:e3:claim:e3"],
                "text": "E3 acceptance fixture.",
                "source": "embedded backend test",
                "metadata": {"requested_by": "choreographer"}
            }]
        }
    })
}

#[tokio::test]
async fn a_project_write_maintains_its_commit_native_bundle() {
    let project = tempfile::tempdir().expect("project");
    let data_dir = project.path().join(".kernel");
    let bundle_path = project.path().join(".kmp/memory.jsonl");
    let commit_native = kmp_embedded::CommitNativeBundle::new(&data_dir, &bundle_path);
    let backend = EmbeddedKernelMcpBackend::open_with_engine_and_commit_native(
        &data_dir,
        None,
        Some(commit_native),
    )
    .expect("embedded backend");
    let server = KernelMcpServer::with_embedded_backend(backend);

    let response = server
        .handle_json_line(&tool_call(1, "kmp_ingest", ingest_arguments()))
        .await
        .expect("response");
    let response: Value = serde_json::from_str(&response).expect("json");
    assert!(response.get("error").is_none(), "{response}");

    let bundle = std::fs::read_to_string(&bundle_path).expect("auto-exported bundle");
    let header = kmp_embedded::verify_bundle(&bundle).expect("verified bundle");
    assert_eq!(header.event_count, 1);
    assert_eq!(header.abouts, ["question:e3"]);
    assert!(kmp_embedded::pending_bundle_exports(&data_dir).is_empty());

    let historical = kmp_mcp::snapshot::read_only(
        &bundle,
        "kmp_inspect",
        json!({"about": "question:e3", "ref": "question:e3:claim:e3"}),
    )
    .await
    .expect("read verified snapshot in isolation");
    assert_eq!(
        historical["result"]["structuredContent"]["object"]["ref"],
        "question:e3:claim:e3"
    );
    let refused = kmp_mcp::snapshot::read_only(&bundle, "kmp_ingest", ingest_arguments())
        .await
        .expect_err("snapshot attachment is read-only");
    assert!(refused.contains("could mutate memory"));
}

#[tokio::test]
async fn a_failed_bundle_refresh_leaves_a_loud_pending_marker() {
    let project = tempfile::tempdir().expect("project");
    let data_dir = project.path().join(".kernel");
    let blocked_parent = project.path().join("blocked");
    std::fs::write(&blocked_parent, b"not a directory").expect("blocker");
    let bundle_path = blocked_parent.join("memory.jsonl");
    let commit_native = kmp_embedded::CommitNativeBundle::new(&data_dir, &bundle_path);
    let backend = EmbeddedKernelMcpBackend::open_with_engine_and_commit_native(
        &data_dir,
        None,
        Some(commit_native),
    )
    .expect("embedded backend");
    let server = KernelMcpServer::with_embedded_backend(backend);

    let response = server
        .handle_json_line(&tool_call(1, "kmp_ingest", ingest_arguments()))
        .await
        .expect("response");
    let response: Value = serde_json::from_str(&response).expect("json");
    assert_eq!(
        response["result"]["structuredContent"]["error"]["code"],
        "backend_error"
    );
    assert!(
        response["result"]["structuredContent"]["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("write committed")),
        "{response}"
    );
    assert_eq!(
        kmp_embedded::pending_bundle_exports(&data_dir).len(),
        1,
        "doctor must have durable evidence that export did not complete"
    );
}

fn large_recall_ingest_arguments() -> Value {
    let mut evidence = (0..301)
        .map(|index| {
            json!({
                "id": format!("evidence:project:large-recall:weak:{index:03}"),
                "supports": ["project:large-recall:claim:large-recall"],
                "text": format!(
                    "Gate deficiencies caused rejection; authority remains withheld for unrelated rollout {index}."
                ),
                "source": format!("historical gate note {index:03}")
            })
        })
        .collect::<Vec<_>>();
    evidence.push(json!({
        "id": "evidence:project:large-recall:exact:gate-rejection",
        "supports": ["project:large-recall:claim:large-recall"],
        "text": "The exact deficiencies caused rejection were missing contract tests; authority remains withheld until the gate passes.",
        "source": "current gate review"
    }));

    json!({
        "about": "project:large-recall",
        "idempotency_key": "ingest:large-recall-ranking",
        "memory": {
            "dimensions": [{"id": "work:large-recall", "kind": "work"}],
            "entries": [
                {
                    "id": "project:large-recall:claim:large-recall",
                    "kind": "claim",
                    "text": "Large recall gate result.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:large-recall",
                        "occurred_at": "2026-08-18T00:00:00Z",
                        "sequence": 1
                    }]
                },
                {
                    "id": "project:large-recall:claim:gate-action",
                    "kind": "claim",
                    "text": "Gate remediation must happen before authority is released.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:large-recall",
                        "occurred_at": "2026-08-18T00:00:01Z",
                        "sequence": 2
                    }]
                }
            ],
            "relations": [{
                "from": "project:large-recall:claim:large-recall",
                "to": "project:large-recall:claim:gate-action",
                "rel": "triggers",
                "class": "causal",
                "why": "The rejected gate caused remediation to become the next required action.",
                "evidence": "The gate review withheld authority until its deficiencies are corrected.",
                "confidence": "high"
            }],
            "evidence": evidence
        }
    })
}

fn paraphrase_recall_ingest_arguments() -> Value {
    json!({
        "about": "project:live-validation",
        "idempotency_key": "ingest:paraphrase-recall-regression",
        "memory": {
            "dimensions": [{"id": "work:live-validation", "kind": "work"}],
            "entries": [
                {
                    "id": "project:live-validation:success:ranking-correction-merged",
                    "kind": "success_path",
                    "text": "The relevance-ranking correction passed its regression test and was merged.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:live-validation",
                        "occurred_at": "2026-08-17T23:55:00Z",
                        "sequence": 1
                    }]
                },
                {
                    "id": "project:live-validation:constraint:restart-live-service",
                    "kind": "constraint",
                    "text": "The connected live service must be rebuilt or reinstalled from the corrected release and restarted before it can validate the relevance-ranking change.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:live-validation",
                        "occurred_at": "2026-08-18T00:00:00Z",
                        "sequence": 2
                    }]
                },
                {
                    "id": "project:live-validation:success:corrected-service-installed",
                    "kind": "success_path",
                    "text": "The corrected service release is now installed for future launches.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:live-validation",
                        "occurred_at": "2026-08-18T00:05:00Z",
                        "sequence": 3
                    }]
                },
                {
                    "id": "project:live-validation:error:tls-projection-race",
                    "kind": "error_path",
                    "text": "A TLS projection test observed a partial graph.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:live-validation",
                        "occurred_at": "2026-08-18T00:10:00Z",
                        "sequence": 4
                    }]
                }
            ],
            "relations": [
                {
                    "from": "project:live-validation:constraint:restart-live-service",
                    "to": "project:live-validation:success:ranking-correction-merged",
                    "rel": "depends_on",
                    "class": "causal",
                    "why": "Live validation depends on rebuilding and restarting the service because the running executable predates the corrected relevance-ranking implementation.",
                    "evidence": "The old service reproduced the stale weak-prefix retrieval result after the corrected build passed its regression test.",
                    "confidence": "high"
                },
                {
                    "from": "project:live-validation:success:corrected-service-installed",
                    "to": "project:live-validation:constraint:restart-live-service",
                    "rel": "updates_state",
                    "class": "causal",
                    "why": "Installing the corrected release removes the stale executable for future launches, while an already-running process still requires restart.",
                    "evidence": "The installer replaced the old release with the corrected build; the live process was not restarted.",
                    "confidence": "high"
                },
                {
                    "from": "project:live-validation:error:tls-projection-race",
                    "to": "project:live-validation:constraint:restart-live-service",
                    "rel": "checked_against",
                    "class": "evidential",
                    "why": "The projection race was compared with the live service restart constraint while triaging issue 80.",
                    "evidence": "The comparison mentioned the retrieval regression, required rebuild and restart, and later validation against the live service.",
                    "confidence": "high"
                }
            ],
            "evidence": [
                {
                    "id": "evidence:project:live-validation:old-live-service",
                    "supports": ["project:live-validation:constraint:restart-live-service"],
                    "text": "The running service used an older executable while the corrected repository build was newer. Its live query reproduced the stale weak-prefix result, so the corrected implementation had not yet been validated in that process.",
                    "source": "live validation probe"
                },
                {
                    "id": "evidence:project:live-validation:corrected-install",
                    "supports": ["project:live-validation:success:corrected-service-installed"],
                    "text": "The installer replaced the previous release with the corrected build for subsequent service launches.",
                    "source": "installation verification"
                },
                {
                    "id": "evidence:project:live-validation:tls-projection-race",
                    "supports": ["project:live-validation:error:tls-projection-race"],
                    "text": "The TLS projection test returned seven of seventeen nodes because its wait was missing.",
                    "source": "unrelated projection investigation"
                }
            ]
        }
    })
}

fn graph_reranker_ingest_arguments() -> Value {
    json!({
        "about": "decision:sqlite-wal",
        "idempotency_key": "ingest:graph-reranker-regression",
        "memory": {
            "dimensions": [{"id": "work:graph-reranker", "kind": "work"}],
            "entries": [
                {
                    "id": "decision:sqlite-wal:entry:decision:sqlite-wal",
                    "kind": "decision",
                    "text": "SQLite WAL is the embedded engine for concurrent KMP processes sharing one store.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:graph-reranker",
                        "occurred_at": "2026-08-18T08:00:00Z",
                        "sequence": 1
                    }]
                },
                {
                    "id": "decision:sqlite-wal:constraint:shared-process-store",
                    "kind": "constraint",
                    "text": "Two KMP processes must safely share the same embedded store.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:graph-reranker",
                        "occurred_at": "2026-08-18T08:00:01Z",
                        "sequence": 2
                    }]
                },
                {
                    "id": "decision:sqlite-wal:outcome:migration-replay",
                    "kind": "outcome",
                    "text": "Migration replay preserves existing redb stores while fresh stores use SQLite.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:graph-reranker",
                        "occurred_at": "2026-08-18T08:00:02Z",
                        "sequence": 3
                    }]
                },
                {
                    "id": "decision:sqlite-wal:note:earlier-features",
                    "kind": "note",
                    "text": "KMP shipped more features earlier than one planning note expected.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:graph-reranker",
                        "occurred_at": "2026-08-18T08:00:03Z",
                        "sequence": 4
                    }]
                },
                {
                    "id": "decision:sqlite-wal:note:format-layout",
                    "kind": "note",
                    "text": "KMP format version names use the same directory layout.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:graph-reranker",
                        "occurred_at": "2026-08-18T08:00:04Z",
                        "sequence": 5
                    }]
                },
                {
                    "id": "decision:sqlite-wal:note:release-verification",
                    "kind": "note",
                    "text": "One local KMP release was verified after installation.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:graph-reranker",
                        "occurred_at": "2026-08-18T08:00:05Z",
                        "sequence": 6
                    }]
                }
            ],
            "relations": [
                {
                    "from": "decision:sqlite-wal:entry:decision:sqlite-wal",
                    "to": "decision:sqlite-wal:constraint:shared-process-store",
                    "rel": "chosen_because",
                    "class": "motivational",
                    "why": "SQLite WAL replaced redb because concurrent processes need one shareable embedded store.",
                    "evidence": "The multi-process architecture decision records the engine comparison and concurrency requirement.",
                    "confidence": "high"
                },
                {
                    "from": "decision:sqlite-wal:outcome:migration-replay",
                    "to": "decision:sqlite-wal:entry:decision:sqlite-wal",
                    "rel": "depends_on",
                    "class": "causal",
                    "why": "Replay preserves legacy redb data while the SQLite decision governs fresh shared stores.",
                    "evidence": "Migration compatibility and the current engine decision were verified together.",
                    "confidence": "high"
                }
            ],
            "evidence": [
                {
                    "id": "evidence:decision:sqlite-wal:sqlite-wal-decision",
                    "supports": ["decision:sqlite-wal:entry:decision:sqlite-wal"],
                    "text": "SQLite WAL is the embedded storage engine selected for concurrent KMP processes sharing one store.",
                    "source": "ADR fixture"
                },
                {
                    "id": "evidence:decision:sqlite-wal:shared-process-constraint",
                    "supports": ["decision:sqlite-wal:constraint:shared-process-store"],
                    "text": "Two KMP processes require a shareable embedded store.",
                    "source": "concurrency fixture"
                },
                {
                    "id": "evidence:decision:sqlite-wal:migration-replay",
                    "supports": ["decision:sqlite-wal:outcome:migration-replay"],
                    "text": "Existing redb stores remain readable during migration; new stores select SQLite.",
                    "source": "migration fixture"
                },
                {
                    "id": "evidence:decision:sqlite-wal:earlier-features",
                    "supports": ["decision:sqlite-wal:note:earlier-features"],
                    "text": "KMP added more features earlier than one roadmap expected.",
                    "source": "weak lexical distractor"
                },
                {
                    "id": "evidence:decision:sqlite-wal:format-layout",
                    "supports": ["decision:sqlite-wal:note:format-layout"],
                    "text": "KMP version names use the same format layout.",
                    "source": "weak lexical distractor"
                },
                {
                    "id": "evidence:decision:sqlite-wal:release-verification",
                    "supports": ["decision:sqlite-wal:note:release-verification"],
                    "text": "One KMP release was verified after installation.",
                    "source": "weak lexical distractor"
                }
            ]
        }
    })
}

fn partial_default_update_ingest_arguments() -> Value {
    json!({
        "about": "decision:fresh-store-default",
        "idempotency_key": "ingest:partial-default-update-eval",
        "memory": {
            "dimensions": [{"id": "work:fresh-store-default", "kind": "work"}],
            "entries": [
                {
                    "id": "decision:fresh-store-default:decision:two-engine-architecture",
                    "kind": "decision",
                    "text": "KMP keeps redb compatibility and SQLite as two embedded storage engines.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:fresh-store-default",
                        "occurred_at": "2026-08-17T11:37:00Z",
                        "sequence": 1
                    }]
                },
                {
                    "id": "decision:fresh-store-default:decision:historical-fresh-store-default",
                    "kind": "decision",
                    "text": "Redb was the distribution default for a fresh KMP data directory.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:fresh-store-default",
                        "occurred_at": "2026-08-17T11:38:00Z",
                        "sequence": 2
                    }]
                },
                {
                    "id": "decision:fresh-store-default:decision:current-fresh-store-default",
                    "kind": "decision",
                    "text": "Shipped KMP builds create fresh SQLite stores while preserving existing redb stores.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:fresh-store-default",
                        "occurred_at": "2026-08-17T11:39:00Z",
                        "sequence": 3
                    }]
                }
            ],
            "relations": [
                {
                    "from": "decision:fresh-store-default:decision:current-fresh-store-default",
                    "to": "decision:fresh-store-default:decision:historical-fresh-store-default",
                    "rel": "updates_state",
                    "class": "causal",
                    "why": "The shipped fresh-store policy changes only the distribution default, not the two-engine architecture.",
                    "evidence": "A fresh data directory now selects SQLite while existing redb stores remain readable.",
                    "confidence": "high"
                },
                {
                    "from": "decision:fresh-store-default:decision:current-fresh-store-default",
                    "to": "decision:fresh-store-default:decision:two-engine-architecture",
                    "rel": "uses_background",
                    "class": "evidential",
                    "why": "The new default operates inside the existing two-engine compatibility architecture.",
                    "evidence": "Existing redb stores remain readable while fresh stores select SQLite.",
                    "confidence": "high"
                }
            ],
            "evidence": [
                {
                    "id": "evidence:decision:fresh-store-default:historical-fresh-store-default",
                    "supports": ["decision:fresh-store-default:decision:historical-fresh-store-default"],
                    "text": "Before the distribution change, fresh KMP data directories defaulted to redb.",
                    "source": "historical release fixture",
                    "time": "2026-08-17T11:38:00Z"
                },
                {
                    "id": "evidence:decision:fresh-store-default:current-fresh-store-default",
                    "supports": ["decision:fresh-store-default:decision:current-fresh-store-default"],
                    "text": "Existing redb stores remain readable; a new KMP installation creates a fresh SQLite store by default.",
                    "source": "current release fixture",
                    "time": "2026-08-17T11:39:00Z"
                },
                {
                    "id": "evidence:decision:fresh-store-default:two-engine-architecture",
                    "supports": ["decision:fresh-store-default:decision:two-engine-architecture"],
                    "text": "The architecture retains both the redb compatibility path and the SQLite engine.",
                    "source": "architecture fixture",
                    "time": "2026-08-17T11:37:00Z"
                }
            ]
        }
    })
}

fn relation_why_seed_arguments() -> Value {
    json!({
        "about": "project:relation-why-conformance",
        "idempotency_key": "ingest:relation-why-constraint",
        "memory": {
            "dimensions": [{"id": "work:relation-why", "kind": "work"}],
            "entries": [{
                "id": "project:relation-why-conformance:constraint:share-embedded-store",
                "kind": "constraint",
                "text": "Independent KMP agent processes must safely share one embedded store.",
                "coordinates": [{
                    "dimension": "work",
                    "scope_id": "work:relation-why",
                    "occurred_at": "2026-08-18T09:00:00Z",
                    "sequence": 1
                }]
            }],
            "relations": [],
            "evidence": [{
                "id": "evidence:project:relation-why-conformance:shared-store-requirement",
                "supports": ["project:relation-why-conformance:constraint:share-embedded-store"],
                "text": "The host architecture starts independent KMP processes against the same project store.",
                "source": "host architecture fixture"
            }]
        }
    })
}

fn language_fallback_seed_arguments() -> Value {
    json!({
        "about": "project:language-fallback",
        "idempotency_key": "ingest:language-fallback-evidence",
        "memory": {
            "dimensions": [{"id": "work:language-fallback", "kind": "work"}],
            "entries": [
                {
                    "id": "project:language-fallback:decision:embedded-redb",
                    "kind": "decision",
                    "text": "The embedded store uses redb.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:language-fallback",
                        "occurred_at": "2026-08-24T10:15:00Z",
                        "sequence": 1
                    }]
                },
                {
                    "id": "project:language-fallback:constraint:single-writer-ownership",
                    "kind": "constraint",
                    "text": "One writer matches one agent per project.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:language-fallback",
                        "occurred_at": "2026-08-24T10:16:00Z",
                        "sequence": 2
                    }]
                }
            ],
            "relations": [{
                "from": "project:language-fallback:decision:embedded-redb",
                "to": "project:language-fallback:constraint:single-writer-ownership",
                "rel": "uses_background",
                "class": "evidential",
                "why": "The single-writer model matches the product's per-project agent ownership.",
                "evidence": "ADR-011 records one writer per redb store.",
                "confidence": "high"
            }],
            "evidence": [{
                "id": "evidence:project:language-fallback:embedded-redb-choice",
                "supports": ["project:language-fallback:decision:embedded-redb"],
                "text": "We chose redb because one writer matched one agent per project.",
                "source": "https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/docs/adr/ADR-011-embedded-concurrency-model.md#L42",
                "metadata": {"language": "en", "digest": "sha256:language-fixture"}
            }]
        }
    })
}

fn diacritic_recall_seed_arguments() -> Value {
    let records = [
        (
            "es",
            "La válvula falló por sedimentación.",
            "2026-08-24T11:00:00Z",
        ),
        (
            "pt",
            "A refrigeração parou por calcificação.",
            "2026-08-24T11:00:01Z",
        ),
        ("fr", "L'arrêt venait du dépôt.", "2026-08-24T11:00:02Z"),
        (
            "de",
            "Die Straße blieb gesperrt; das Kühlventil wurde ersetzt.",
            "2026-08-24T11:00:03Z",
        ),
    ];
    let entries = records
        .iter()
        .enumerate()
        .map(|(index, (language, text, occurred_at))| {
            json!({
                "id": format!("project:diacritic-recall:incident:diacritic:{language}"),
                "kind": "incident",
                "text": text,
                "metadata": {"language": language},
                "coordinates": [{
                    "dimension": "work",
                    "scope_id": "work:diacritic-recall",
                    "occurred_at": occurred_at,
                    "sequence": index + 1
                }]
            })
        })
        .collect::<Vec<_>>();
    let evidence = records
        .iter()
        .map(|(language, text, _)| {
            json!({
                "id": format!("evidence:project:diacritic-recall:diacritic:{language}"),
                "supports": [format!("project:diacritic-recall:incident:diacritic:{language}")],
                "text": text,
                "source": format!("{language} incident fixture"),
                "metadata": {"language": language}
            })
        })
        .collect::<Vec<_>>();
    json!({
        "about": "project:diacritic-recall",
        "idempotency_key": "ingest:diacritic-recall-regression",
        "memory": {
            "dimensions": [{"id": "work:diacritic-recall", "kind": "work"}],
            "entries": entries,
            "relations": [],
            "evidence": evidence
        }
    })
}

#[tokio::test]
async fn semantic_language_retry_recovers_english_evidence_without_rewriting_it() {
    const TEXT: &str = "We chose redb because one writer matched one agent per project.";
    const WHY: &str = "The single-writer model matches the product's per-project agent ownership.";
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(&server, 1, "kmp_ingest", language_fallback_seed_arguments()).await;

    let spanish = call(
        &server,
        2,
        "kmp_ask",
        json!({
            "about": "project:language-fallback",
            "question": "¿Por qué un escritor corresponde a un agente por proyecto?",
            "answer_policy": "evidence_or_unknown",
            "budget": {"detail": "full"}
        }),
    )
    .await;
    assert_eq!(spanish["answer"], "UNKNOWN", "{spanish}");

    let english = call(
        &server,
        3,
        "kmp_ask",
        json!({
            "about": "project:language-fallback",
            "question": "Why does one writer correspond to one agent per project?",
            "answer_policy": "evidence_or_unknown",
            "budget": {"detail": "full"}
        }),
    )
    .await;
    assert_ne!(english["answer"], "UNKNOWN", "{english}");
    let evidence = english["proof"]["evidence"]
        .as_array()
        .expect("English retry carries evidence")
        .iter()
        .find(|evidence| {
            evidence["id"] == "detail:evidence:project:language-fallback:embedded-redb-choice"
        })
        .expect("English retry cites the stored evidence");
    assert_eq!(evidence["text"], TEXT);
    assert_eq!(
        evidence["source"],
        "https://github.com/underpass-ai/kmp/blob/v0.5.0/archive/docs/adr/ADR-011-embedded-concurrency-model.md#L42"
    );
    assert_eq!(evidence["metadata"]["language"], "en");
    assert_eq!(evidence["metadata"]["digest"], "sha256:language-fixture");
    assert!(
        english["proof"]["path"]
            .as_array()
            .expect("English retry carries relation context")
            .iter()
            .any(|relation| relation["why"] == WHY),
        "{english}"
    );
}

#[tokio::test]
async fn ask_folds_diacritics_without_rewriting_multilingual_evidence() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(&server, 1, "kmp_ingest", diacritic_recall_seed_arguments()).await;

    for (index, (query, stored)) in [
        ("valvula", "La válvula falló por sedimentación."),
        ("refrigeracao", "A refrigeração parou por calcificação."),
        ("arret", "L'arrêt venait du dépôt."),
        (
            "strasse",
            "Die Straße blieb gesperrt; das Kühlventil wurde ersetzt.",
        ),
        (
            "kuhlventil",
            "Die Straße blieb gesperrt; das Kühlventil wurde ersetzt.",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let answer = call(
            &server,
            index as u64 + 2,
            "kmp_ask",
            json!({
                "about": "project:diacritic-recall",
                "question": query,
                "answer_policy": "evidence_or_unknown",
                "budget": {"detail": "full", "max_bytes": 20_000}
            }),
        )
        .await;
        assert_ne!(answer["answer"], "UNKNOWN", "{query}: {answer}");
        assert!(
            answer["proof"]["evidence"]
                .as_array()
                .expect("answer evidence")
                .iter()
                .any(|evidence| evidence["text"] == stored),
            "{query} did not return byte-exact evidence: {answer}"
        );
    }
}

#[tokio::test]
async fn embedded_backend_round_trips_entry_metadata_and_evidence_source() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(&server, 1, "kmp_ingest", ingest_arguments()).await;

    let goto = call(
        &server,
        2,
        "kmp_goto",
        json!({
            "about": "question:e3",
            "at": {"sequence": 2},
            "include": {"evidence": true, "relations": true}
        }),
    )
    .await;
    assert_eq!(goto["page"]["returned"], 2);
    assert_eq!(goto["page"]["has_more"], false);
    assert_eq!(
        goto["quality"]["causal_density"]
            .as_f64()
            .expect("goto quality has causal density"),
        0.0,
        "evidential relations must not inflate causal density"
    );
    let entry = goto["entries"]
        .as_array()
        .expect("goto entries are an array")
        .iter()
        .find(|entry| entry["ref"] == "question:e3:claim:e3")
        .expect("goto returns the first claim");
    assert_eq!(entry["metadata"]["window"], "10:00-10:20");
    assert_eq!(entry["metadata"]["probe_digest"], "sha256:e3");

    let evidence = goto["proof"]["evidence"]
        .as_array()
        .expect("goto proof evidence is an array")
        .iter()
        .find(|evidence| evidence["id"] == "detail:evidence:question:e3:claim:e3:fixture")
        .expect("goto returns explicit evidence");
    assert_eq!(evidence["source"], "embedded backend test");
    assert_eq!(evidence["supports"], json!(["question:e3:claim:e3"]));
    assert_eq!(evidence["metadata"]["requested_by"], "choreographer");

    let relation = goto["proof"]["path"]
        .as_array()
        .expect("goto proof path is an array")
        .iter()
        .find(|relation| {
            relation["from"] == "question:e3:claim:e3"
                && relation["to"] == "question:e3:claim:e3-detail"
        })
        .expect("goto returns the qualified relation");
    assert_eq!(relation["motivation"], "Preserve the acceptance rationale.");
    assert_eq!(relation["method"], "Embedded conformance probe.");
    assert_eq!(relation["decision_id"], "question:e3:decision:e3");
    assert_eq!(relation["caused_by_node_id"], "question:e3:claim:e3");
    assert_eq!(relation["coordinate"]["dimension"], "conversation");
    assert_eq!(
        relation["coordinate"]["valid_until"],
        "2026-07-22T10:20:00Z"
    );

    let wake = call(&server, 5, "kmp_wake", json!({"about": "question:e3"})).await;
    let wake_evidence = wake["proof"]["evidence"]
        .as_array()
        .expect("wake proof evidence is an array")
        .iter()
        .find(|evidence| evidence["id"] == "detail:evidence:question:e3:claim:e3:fixture")
        .expect("wake returns explicit evidence");
    assert_eq!(wake_evidence["source"], "embedded backend test");

    let ask = call(
        &server,
        6,
        "kmp_ask",
        json!({"about": "question:e3", "question": "What was accepted?"}),
    )
    .await;
    let ask_evidence = ask["proof"]["evidence"]
        .as_array()
        .expect("ask proof evidence is an array")
        .iter()
        .find(|evidence| evidence["id"] == "detail:evidence:question:e3:claim:e3:fixture")
        .expect("ask returns explicit evidence");
    assert_eq!(ask_evidence["source"], "embedded backend test");

    let inspected_entry = call(
        &server,
        3,
        "kmp_inspect",
        json!({"about": "question:e3", "ref": "question:e3:claim:e3"}),
    )
    .await;
    assert_eq!(
        inspected_entry["object"]["metadata"]["window"],
        "10:00-10:20"
    );
    assert_eq!(
        inspected_entry["links"]["outgoing"][0]["to"], "question:e3:claim:e3-detail",
        "default inspect must return direct links"
    );

    let inspected_evidence = call(
        &server,
        4,
        "kmp_inspect",
        json!({"about": "question:e3", "ref": "evidence:question:e3:claim:e3:fixture"}),
    )
    .await;
    assert_eq!(
        inspected_evidence["object"]["source"],
        "embedded backend test"
    );
    assert_eq!(
        inspected_evidence["object"]["metadata"]["requested_by"],
        "choreographer"
    );
}

#[tokio::test]
async fn partial_goto_and_near_name_the_moves_that_can_continue_them() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(&server, 1, "kmp_ingest", ingest_arguments()).await;

    let goto = call(
        &server,
        2,
        "kmp_goto",
        json!({
            "about": "question:e3",
            "at": {"sequence": 2},
            "limit": {"entries": 1}
        }),
    )
    .await;
    assert_eq!(goto["page"]["has_more"], true, "{goto}");
    assert!(
        goto["next_action"]
            .as_str()
            .is_some_and(|action| action.contains("kmp_rewind")
                && action.contains("Do not pass this cursor back to kmp_goto")),
        "{goto}"
    );

    let near = call(
        &server,
        3,
        "kmp_near",
        json!({
            "about": "question:e3",
            "around": {"sequence": 2},
            "window": {"before_entries": 0, "after_entries": 0}
        }),
    )
    .await;
    assert_eq!(near["page"]["has_more"], true, "{near}");
    let action = near["next_action"].as_str().expect("near next action");
    assert!(action.contains("kmp_rewind"), "{near}");
    assert!(action.contains("kmp_forward"), "{near}");
    assert!(
        action.contains("Do not pass page.next_cursor back to kmp_near"),
        "{near}"
    );
}

#[tokio::test]
async fn large_recall_keeps_the_strongest_answer_and_semantic_wake_state() {
    const TOKEN_LIMIT: u32 = 3_000;
    const EXACT_ANSWER: &str = "The exact deficiencies caused rejection were missing contract tests; authority remains withheld until the gate passes.";
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(&server, 1, "kmp_ingest", large_recall_ingest_arguments()).await;
    let budget = json!({
        "tokens": TOKEN_LIMIT,
        "detail": "balanced",
        "max_entries": 12
    });

    let ask = call(
        &server,
        2,
        "kmp_ask",
        json!({
            "about": "project:large-recall",
            "question": "What exact deficiencies caused rejection and what authority remains withheld?",
            "answer_policy": "evidence_or_unknown",
            "depth": 3,
            "budget": budget
        }),
    )
    .await;
    assert!(ask["because"][0].get("evidence").is_none(), "{ask}");
    let cited_ref = ask["because"][0]["ref"].as_str().expect("evidence ref");
    assert!(
        ask["proof"]["evidence"]
            .as_array()
            .expect("canonical evidence")
            .iter()
            .any(|evidence| evidence["id"] == cited_ref && evidence["text"] == EXACT_ANSWER),
        "{ask}"
    );
    assert_eq!(ask["proof"]["confidence"], "high");
    assert_eq!(ask["truncation"]["truncated"], true);
    assert!(
        ask["truncation"]["omitted"]
            .as_object()
            .is_some_and(|omitted| omitted
                .values()
                .any(|count| count.as_u64().unwrap_or(0) > 0)),
        "{ask}"
    );

    let wake = call(
        &server,
        3,
        "kmp_wake",
        json!({
            "about": "project:large-recall",
            "intent": "continue gate remediation",
            "depth": 3,
            "budget": {
                "tokens": TOKEN_LIMIT,
                "detail": "balanced",
                "max_entries": 12
            }
        }),
    )
    .await;
    assert_eq!(
        wake["wake"]["next_actions"][0], "triggers → project:large-recall:claim:gate-action",
        "{wake}"
    );
    assert_eq!(
        wake["wake"]["causal_spine"][0]["because"],
        "The rejected gate caused remediation to become the next required action."
    );

    let estimator = Cl100kEstimator::new();
    for response in [&ask, &wake] {
        assert!(
            estimator.estimate_tokens(&response.to_string()) <= TOKEN_LIMIT,
            "structured response exceeded the hard token ceiling: {response}"
        );
    }
}

#[tokio::test]
async fn ask_recalls_supported_constraint_across_morphology_and_clause_reordering() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(
        &server,
        1,
        "kmp_ingest",
        paraphrase_recall_ingest_arguments(),
    )
    .await;

    let questions = [
        "What retrieval failure did the old service reproduce, and why must it be rebuilt and restarted before validating the correction?",
        "Why is a reinstall plus process restart required before the fixed relevance ranking can be checked against the live service, and which stale result came from the prior executable?",
    ];

    for (question_index, question) in questions.into_iter().enumerate() {
        let mut first = None;
        for repeat in 0..3 {
            let ask = call(
                &server,
                2 + (question_index * 3 + repeat) as u64,
                "kmp_ask",
                json!({
                    "about": "project:live-validation",
                    "question": question,
                    "answer_policy": "evidence_or_unknown",
                    "depth": 3,
                    "budget": {
                        "tokens": 2_048,
                        "detail": "balanced",
                        "max_entries": 10
                    }
                }),
            )
            .await;

            assert_ne!(ask["answer"], "UNKNOWN", "{question}: {ask}");
            let answer_context = ask["proof"]["evidence"]
                .as_array()
                .expect("canonical answer evidence")
                .iter()
                .filter_map(|evidence| evidence["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                answer_context.contains("stale weak-prefix result"),
                "{question}: {ask}"
            );
            assert!(
                !answer_context.contains("TLS projection"),
                "relation context must not make an unrelated citation eligible: {ask}"
            );
            assert!(
                ask["because"]
                    .as_array()
                    .expect("answer citations")
                    .iter()
                    .all(|reason| reason["ref"].as_str().is_some_and(|evidence_ref| {
                        ask["answer"]
                            .as_str()
                            .is_some_and(|answer| answer.contains(evidence_ref))
                            && ask["proof"]["evidence"]
                                .as_array()
                                .expect("canonical answer evidence")
                                .iter()
                                .any(|evidence| evidence["id"] == evidence_ref)
                    })),
                "answer citations must resolve inside the packet: {ask}"
            );
            assert!(
                ask["proof"]["path"]
                    .as_array()
                    .is_some_and(|path| path.iter().any(|relation| {
                        relation["rel"] == "depends_on" || relation["rel"] == "updates_state"
                    })),
                "the proof must retain the semantic relation that makes the paraphrase auditable: {ask}"
            );
            assert!(
                ask["proof"]["matched_terms"]
                    .as_array()
                    .is_some_and(|terms| terms.len() >= 4),
                "proof must explain eligibility with retained query terms: {ask}"
            );
            assert!(
                ask["proof"]["matched_relations"]
                    .as_array()
                    .is_some_and(|relations| relations.iter().any(|relation| {
                        relation == "depends_on" || relation == "updates_state"
                    })),
                "proof must identify the semantic relation used for recall: {ask}"
            );

            if let Some(first) = &first {
                assert_eq!(ask, *first, "identical asks must be deterministic");
            } else {
                first = Some(ask);
            }
        }
    }

    let unrelated = call(
        &server,
        10,
        "kmp_ask",
        json!({
            "about": "project:live-validation",
            "question": "Which catering vendor supplies the launch dinner?",
            "answer_policy": "evidence_or_unknown",
            "depth": 3
        }),
    )
    .await;
    assert_eq!(unrelated["answer"], "UNKNOWN", "{unrelated}");
    assert_eq!(unrelated["proof"]["matched_terms"], json!([]));
    assert_eq!(unrelated["proof"]["matched_relations"], json!([]));
}

#[tokio::test]
async fn graph_aware_reranker_keeps_answer_claims_ahead_of_weak_novelty() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    let ingest = call(&server, 1, "kmp_ingest", graph_reranker_ingest_arguments()).await;
    assert_eq!(
        ingest["memory"]["about"], "decision:sqlite-wal",
        "fixture ingest failed: {ingest}"
    );
    let arguments = json!({
        "about": "decision:sqlite-wal",
        "question": "Which embedded storage engine should two KMP processes sharing one store use, and which engine did it replace?",
        "answer_policy": "evidence_or_unknown",
        "depth": 3,
        "budget": {
            "detail": "balanced",
            "max_entries": 12,
            "max_bytes": 10_000
        }
    });

    let mut first = None;
    for repeat in 0..3 {
        let ask = call(&server, 2 + repeat, "kmp_ask", arguments.clone()).await;
        assert_ne!(ask["answer"], "UNKNOWN", "{ask}");
        let evidence_ids = ask["proof"]["evidence"]
            .as_array()
            .unwrap_or_else(|| panic!("canonical answer evidence: {ask}"))
            .iter()
            .filter_map(|evidence| evidence["id"].as_str())
            .collect::<Vec<_>>();
        assert!(
            evidence_ids.contains(&"detail:evidence:decision:sqlite-wal:sqlite-wal-decision"),
            "the answer-bearing architecture decision must survive reranking: {ask}"
        );
        assert!(
            ask["because"]
                .as_array()
                .expect("answer citations")
                .iter()
                .any(|reason| {
                    reason["claim"] == "decision:sqlite-wal:entry:decision:sqlite-wal"
                }),
            "the scarce answer core must cite the architecture decision: {ask}"
        );
        assert!(
            ask["proof"]["matched_relations"]
                .as_array()
                .is_some_and(|relations| relations.iter().any(|rel| rel == "chosen_because")),
            "the relation why contribution must remain auditable: {ask}"
        );
        if let Some(first) = &first {
            assert_eq!(ask, *first, "fresh calls must be byte-stable in structure");
        } else {
            first = Some(ask);
        }
    }
}

#[tokio::test]
async fn current_default_recall_survives_a_partial_decision_update() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(
        &server,
        1,
        "kmp_ingest",
        partial_default_update_ingest_arguments(),
    )
    .await;

    let questions = [
        "What is the current default storage engine for a fresh KMP data directory?",
        "Which backend will a new installation select when no existing store is present?",
    ];
    for (question_index, question) in questions.into_iter().enumerate() {
        let mut first = None;
        for repeat in 0..3 {
            let ask = call(
                &server,
                2 + (question_index * 3 + repeat) as u64,
                "kmp_ask",
                json!({
                    "about": "decision:fresh-store-default",
                    "question": question,
                    "answer_policy": "evidence_or_unknown",
                    "depth": 3,
                    "budget": {"detail": "balanced", "max_bytes": 10_000}
                }),
            )
            .await;

            assert_ne!(ask["answer"], "UNKNOWN", "{question}: {ask}");
            assert_eq!(
                ask["proof"]["evidence"][0]["supports"][0],
                "decision:fresh-store-default:decision:current-fresh-store-default",
                "the current policy must rank first: {ask}"
            );
            assert!(
                ask["proof"]["evidence"][0]["text"]
                    .as_str()
                    .is_some_and(|text| text.contains("SQLite")),
                "the current answer evidence must name SQLite: {ask}"
            );
            assert!(
                ask["proof"]["matched_relations"]
                    .as_array()
                    .is_some_and(|relations| relations.iter().any(|rel| rel == "updates_state")),
                "the partial update must remain visible in proof: {ask}"
            );
            assert!(ask["truncation"]["truncated"].is_boolean(), "{ask}");
            assert!(
                ask["projection"]["budget"]["used_bytes"]
                    .as_u64()
                    .is_some_and(|used| used <= 10_000),
                "the recorded projection must stay inside its byte budget: {ask}"
            );

            if let Some(first) = &first {
                assert_eq!(ask, *first, "identical asks must be deterministic");
            } else {
                first = Some(ask);
            }
        }
    }

    let before = call(
        &server,
        8,
        "kmp_goto",
        json!({
            "about": "decision:fresh-store-default",
            "at": {"sequence": 2},
            "include": {"evidence": true, "relations": true}
        }),
    )
    .await;
    let before_refs = before["entries"]
        .as_array()
        .expect("goto entries")
        .iter()
        .filter_map(|entry| entry["ref"].as_str())
        .collect::<Vec<_>>();
    assert!(
        before_refs
            .contains(&"decision:fresh-store-default:decision:historical-fresh-store-default")
    );
    assert!(
        !before_refs.contains(&"decision:fresh-store-default:decision:current-fresh-store-default")
    );

    let after = call(
        &server,
        9,
        "kmp_goto",
        json!({
            "about": "decision:fresh-store-default",
            "at": {"sequence": 3},
            "include": {"evidence": true, "relations": true}
        }),
    )
    .await;
    assert!(
        after["entries"]
            .as_array()
            .expect("goto entries")
            .iter()
            .any(|entry| {
                entry["ref"] == "decision:fresh-store-default:decision:current-fresh-store-default"
            }),
        "{after}"
    );

    let architecture = call(
        &server,
        10,
        "kmp_inspect",
        json!({
            "about": "decision:fresh-store-default",
            "ref": "decision:fresh-store-default:decision:two-engine-architecture"
        }),
    )
    .await;
    assert!(architecture.to_string().contains("uses_background"));
    assert!(!architecture.to_string().contains("supersedes"));

    let unrelated = call(
        &server,
        11,
        "kmp_ask",
        json!({
            "about": "decision:fresh-store-default",
            "question": "Which catering vendor supplies the launch dinner?",
            "answer_policy": "evidence_or_unknown"
        }),
    )
    .await;
    assert_eq!(unrelated["answer"], "UNKNOWN", "{unrelated}");
}

#[tokio::test]
async fn writer_relation_why_survives_paraphrased_recall_and_audit() {
    const WHY: &str = "SQLite WAL was chosen because independent KMP agents must concurrently share one embedded store.";
    const RELATION_EVIDENCE: &str = "The two-process integration test passed concurrent reads and writes under SQLite WAL and failed at redb's single-writer process lock.";

    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(&server, 1, "kmp_ingest", relation_why_seed_arguments()).await;

    let inspected = call(
        &server,
        2,
        "kmp_inspect",
        json!({
            "about": "project:relation-why-conformance",
            "ref": "project:relation-why-conformance:constraint:share-embedded-store"
        }),
    )
    .await;
    assert_eq!(
        inspected["object"]["text"],
        "Independent KMP agent processes must safely share one embedded store."
    );

    let write_arguments = |dry_run| {
        json!({
            "about": "project:relation-why-conformance",
            "intent": "record_decision",
            "actor": "agent:relation-why-conformance",
            "observed_at": "2026-08-18T09:05:00Z",
            "scope": {"process": "work:relation-why"},
            "current": {
                "ref": "project:relation-why-conformance:decision:sqlite-wal-shared-store",
                "kind": "decision",
                "summary": "Use SQLite WAL instead of redb for shared embedded storage.",
                "evidence": "The architecture comparison selected SQLite WAL over redb after exercising two independent processes."
            },
            "connect_to": [{
                "ref": "project:relation-why-conformance:constraint:share-embedded-store",
                "rel": "chosen_because",
                "class": "motivational",
                "why": WHY,
                "evidence": RELATION_EVIDENCE,
                "confidence": "high"
            }],
            "read_context": {
                "inspected_refs": ["project:relation-why-conformance:constraint:share-embedded-store"]
            },
            "idempotency_key": "write:relation-why-sqlite-decision",
            "options": {"dry_run": dry_run, "strict": true, "sequence": 2}
        })
    };

    let preview = call(&server, 3, "kmp_write_memory", write_arguments(true)).await;
    assert_eq!(preview["accepted"], false);
    assert_eq!(preview["dry_run"], true);
    assert_eq!(
        preview["ingest_preview"]["memory"]["relations"][0]["why"],
        WHY
    );
    assert_eq!(
        preview["ingest_preview"]["memory"]["relations"][0]["evidence"],
        RELATION_EVIDENCE
    );

    let committed = call(&server, 4, "kmp_write_memory", write_arguments(false)).await;
    assert_eq!(committed["accepted"], true, "{committed}");

    let wake = call(
        &server,
        5,
        "kmp_wake",
        json!({"about": "project:relation-why-conformance"}),
    )
    .await;
    assert!(
        wake.to_string().contains(WHY),
        "wake must recover the supplied rationale: {wake}"
    );

    let ask = call(
        &server,
        6,
        "kmp_ask",
        json!({
            "about": "project:relation-why-conformance",
            "question": "Which embedded storage engine should independent KMP processes sharing one store use, and why was redb replaced?",
            "answer_policy": "evidence_or_unknown",
            "depth": 3,
            "budget": {"detail": "full", "max_bytes": 10_000}
        }),
    )
    .await;
    assert_ne!(ask["answer"], "UNKNOWN", "{ask}");
    assert!(
        ask["because"]
            .as_array()
            .is_some_and(|reasons| reasons.iter().any(|reason| {
                reason["claim"]
                    == "project:relation-why-conformance:decision:sqlite-wal-shared-store"
            })),
        "paraphrased recall must retain the decision citation: {ask}"
    );
    assert!(
        ask["proof"]["matched_relations"]
            .as_array()
            .is_some_and(|relations| relations.iter().any(|rel| rel == "chosen_because")),
        "recall must disclose that relation context contributed: {ask}"
    );

    let trace = call(
        &server,
        7,
        "kmp_trace",
        json!({
            "about": "project:relation-why-conformance",
            "from": "project:relation-why-conformance:decision:sqlite-wal-shared-store",
            "to": "project:relation-why-conformance:constraint:share-embedded-store"
        }),
    )
    .await;
    assert!(trace.to_string().contains(WHY), "{trace}");
    assert!(trace.to_string().contains(RELATION_EVIDENCE), "{trace}");

    let relation_proof = call(
        &server,
        8,
        "kmp_inspect",
        json!({
            "about": "project:relation-why-conformance",
            "ref": "evidence:project:relation-why-conformance:decision:sqlite-wal-shared-store:relation:1"
        }),
    )
    .await;
    assert!(
        relation_proof.to_string().contains(RELATION_EVIDENCE),
        "{relation_proof}"
    );
}

#[tokio::test]
async fn generated_writer_refs_keep_repeated_and_long_prefix_memories_distinct() {
    const ROUTINE_SUMMARY: &str = "la presion del circuito es normal";
    const MORNING_EVIDENCE: &str = "manometro a las 09:00 marca 4.1 bar";
    const AFTERNOON_EVIDENCE: &str = "manometro a las 17:00 marca 4.0 bar";
    const SUCCESS_SUMMARY: &str = "el despliegue de la version 2.4.1 en el entorno de preproduccion ha terminado con exito y sin incidencias";
    const FAILURE_SUMMARY: &str = "el despliegue de la version 2.4.1 en el entorno de preproduccion ha terminado con errores graves de arranque";
    const SUCCESS_EVIDENCE: &str = "CI job 4001: exit 0";
    const FAILURE_EVIDENCE: &str = "CI job 4002: exit 1, panic en el arranque";

    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    let record = |summary: &str, evidence: &str, occurred_at: &str| {
        json!({
            "about": "incident:writer-ref-collision",
            "intent": "record_observation",
            "actor": "agent:patrol",
            "observed_at": "2026-08-18T18:00:00Z",
            "occurred_at": occurred_at,
            "scope": {"process": "incident:writer-ref-collision:patrol"},
            "current": {
                "kind": "observation",
                "summary": summary,
                "evidence": evidence
            },
            "options": {"strict": false}
        })
    };

    let writes = [
        (ROUTINE_SUMMARY, MORNING_EVIDENCE, "2026-08-18T09:00:00Z"),
        (ROUTINE_SUMMARY, AFTERNOON_EVIDENCE, "2026-08-18T17:00:00Z"),
        (SUCCESS_SUMMARY, SUCCESS_EVIDENCE, "2026-08-18T17:10:00Z"),
        (FAILURE_SUMMARY, FAILURE_EVIDENCE, "2026-08-18T17:20:00Z"),
    ];
    let mut generated = Vec::new();
    for (index, (summary, evidence, occurred_at)) in writes.iter().enumerate() {
        let response = call(
            &server,
            (index + 1) as u64,
            "kmp_write_memory",
            record(summary, evidence, occurred_at),
        )
        .await;
        assert_eq!(response["accepted"], true, "{response}");
        generated.push(
            response["generated_refs"][0]
                .as_str()
                .expect("generated current ref")
                .to_string(),
        );
    }

    assert_ne!(
        generated[0], generated[1],
        "repeated readings are two facts"
    );
    assert_ne!(
        generated[2], generated[3],
        "opposite statements after a shared long prefix are two facts"
    );

    for (index, ((summary, evidence, _), generated_ref)) in
        writes.iter().zip(&generated).enumerate()
    {
        let inspected = call(
            &server,
            10 + index as u64,
            "kmp_inspect",
            json!({"about": "incident:writer-ref-collision", "ref": generated_ref}),
        )
        .await;
        assert_eq!(inspected["object"]["text"], *summary, "{inspected}");
        assert!(
            inspected.to_string().contains(evidence),
            "each generated ref must retain its own evidence: {inspected}"
        );
    }
}

#[tokio::test]
async fn writer_supplied_refs_cannot_escape_their_about_or_replace_another_root() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    let record = |about: &str, summary: &str, evidence: &str| {
        json!({
            "about": about,
            "intent": "record_observation",
            "actor": "agent:boundary-test",
            "observed_at": "2026-08-18T18:00:00Z",
            "scope": {"process": format!("{about}:patrol")},
            "current": {
                "kind": "observation",
                "summary": summary,
                "evidence": evidence
            },
            "options": {"strict": false}
        })
    };

    let alpha = call(
        &server,
        1,
        "kmp_write_memory",
        record(
            "incident:alpha",
            "la bomba hidraulica perdio presion en el muelle norte",
            "manometro del muelle norte",
        ),
    )
    .await;
    let beta = call(
        &server,
        2,
        "kmp_write_memory",
        record(
            "incident:beta",
            "el firmware del ascensor caduco durante el invierno",
            "informe del controlador del ascensor",
        ),
    )
    .await;
    assert_eq!(alpha["accepted"], true, "{alpha}");
    assert_eq!(beta["accepted"], true, "{beta}");
    let alpha_ref = alpha["generated_refs"][0]
        .as_str()
        .expect("alpha generated ref");
    let beta_ref = beta["generated_refs"][0]
        .as_str()
        .expect("beta generated ref");

    let foreign_inspect = call(
        &server,
        5,
        "kmp_inspect",
        json!({"about": "incident:alpha", "ref": beta_ref}),
    )
    .await;
    assert_eq!(
        foreign_inspect["error"]["code"], "invalid_argument",
        "inspect must enforce its declared about before reading: {foreign_inspect}"
    );
    assert!(
        !foreign_inspect
            .to_string()
            .contains("firmware del ascensor"),
        "a rejected inspection must not narrate the foreign object: {foreign_inspect}"
    );

    let foreign_trace = call(
        &server,
        6,
        "kmp_trace",
        json!({
            "about": "incident:alpha",
            "from": alpha_ref,
            "to": beta_ref
        }),
    )
    .await;
    assert_eq!(
        foreign_trace["error"]["code"], "invalid_argument",
        "trace must reject a foreign endpoint before path rendering: {foreign_trace}"
    );
    assert!(
        !foreign_trace.to_string().contains("firmware del ascensor"),
        "a rejected trace must not leak the foreign target summary: {foreign_trace}"
    );

    let forbidden_refs = [
        beta_ref,
        "incident:beta",
        "about:incident:beta:dimension:patrol",
        "../../incident:beta:entry:x",
        "incident:alpha:entry:x\nincident:beta:entry:y",
    ];
    let mut request_id = 10;
    for field in ["current", "semantic_delta"] {
        for forbidden_ref in forbidden_refs {
            let mut attack = record(
                "incident:alpha",
                "texto plantado desde alpha que no deberia vivir en beta",
                "escritura hecha desde el about alpha",
            );
            if field == "semantic_delta" {
                attack["intent"] = json!("record_delta");
                attack["semantic_delta"] = json!({
                    "ref": forbidden_ref,
                    "from": "estado anterior del muelle",
                    "to": "estado plantado desde alpha",
                    "why": "vector de regresion del limite entre abouts",
                    "evidence": "la llamada declara incident:alpha"
                });
            } else {
                attack["current"]["ref"] = json!(forbidden_ref);
            }
            let refused = call(&server, request_id, "kmp_write_memory", attack).await;
            request_id += 1;
            assert_eq!(refused["error"]["code"], "invalid_argument", "{refused}");
            assert!(
                refused["error"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains(&format!("{field}.ref"))),
                "the refusal must name the supplied field: {refused}"
            );
        }
    }

    let beta_entry = call(
        &server,
        30,
        "kmp_inspect",
        json!({"about": "incident:beta", "ref": beta_ref}),
    )
    .await;
    assert_eq!(
        beta_entry["object"]["text"], "el firmware del ascensor caduco durante el invierno",
        "the rejected alpha writes must leave beta byte-for-byte addressable: {beta_entry}"
    );
    let beta_anchor = call(
        &server,
        31,
        "kmp_inspect",
        json!({"about": "incident:beta", "ref": "incident:beta"}),
    )
    .await;
    assert_eq!(
        beta_anchor["object"]["kind"], "memory_anchor",
        "{beta_anchor}"
    );
}

#[tokio::test]
async fn raw_ingest_refs_cannot_escape_their_about_or_replace_another_root() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    let record = |about: &str, summary: &str| {
        json!({
            "about": about,
            "intent": "record_observation",
            "actor": "agent:ingest-boundary-test",
            "observed_at": "2026-08-28T07:00:00Z",
            "scope": {"process": format!("{about}:patrol")},
            "current": {
                "kind": "observation",
                "summary": summary,
                "evidence": format!("seed evidence for {about}")
            },
            "options": {"strict": false}
        })
    };

    let beta = call(
        &server,
        1,
        "kmp_write_memory",
        record("incident:beta", "beta keeps its own root anchor"),
    )
    .await;
    let gamma = call(
        &server,
        2,
        "kmp_write_memory",
        record("incident:gamma", "gamma keeps its original memory"),
    )
    .await;
    assert_eq!(beta["accepted"], true, "{beta}");
    assert_eq!(gamma["accepted"], true, "{gamma}");
    let gamma_ref = gamma["generated_refs"][0]
        .as_str()
        .expect("gamma generated ref")
        .to_string();

    let hostile_refs = [
        gamma_ref.as_str(),
        "incident:beta",
        "incident:alfa:entry:x\nincident:beta:entry:y",
        "../../incident:beta:entry:x",
    ];
    for (index, hostile_ref) in hostile_refs.into_iter().enumerate() {
        let refused = call(
            &server,
            10 + index as u64,
            "kmp_ingest",
            json!({
                "about": "incident:alfa",
                "idempotency_key": format!("ingest:boundary-probe:{index}"),
                "memory": {
                    "dimensions": [{"id": "incident:alfa:patrol", "kind": "agentic_process"}],
                    "entries": [{
                        "id": hostile_ref,
                        "kind": "observation",
                        "text": "planted from the wrong about",
                        "coordinates": [{
                            "dimension": "agentic_process",
                            "scope_id": "incident:alfa:patrol",
                            "occurred_at": "2026-08-28T07:00:00Z"
                        }]
                    }],
                    "relations": [],
                    "evidence": []
                }
            }),
        )
        .await;
        assert_eq!(refused["error"]["code"], "invalid_argument", "{refused}");
        assert!(
            refused["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("memory.entries[].id")),
            "the refusal must identify the raw entry id: {refused}"
        );
    }

    let gamma_entry = call(
        &server,
        20,
        "kmp_inspect",
        json!({"about": "incident:gamma", "ref": gamma_ref}),
    )
    .await;
    assert_eq!(
        gamma_entry["object"]["text"], "gamma keeps its original memory",
        "rejected ingest calls must not overwrite gamma: {gamma_entry}"
    );
    let beta_anchor = call(
        &server,
        21,
        "kmp_inspect",
        json!({"about": "incident:beta", "ref": "incident:beta"}),
    )
    .await;
    assert_eq!(
        beta_anchor["object"]["kind"], "memory_anchor",
        "rejected ingest calls must not demote beta's root: {beta_anchor}"
    );
}

#[tokio::test]
async fn embedded_backend_returns_structured_not_found_errors() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    let error = call(
        &server,
        1,
        "kmp_goto",
        json!({
            "about": "question:unknown",
            "at": {"sequence": 1}
        }),
    )
    .await;

    assert_eq!(error["error"]["code"], "not_found");
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("structured error includes a message")
            .contains("not found")
    );
}

#[tokio::test]
async fn inspect_negotiates_an_oversized_result_and_only_errors_below_the_object_floor() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    let evidence = (0..18)
        .map(|index| {
            json!({
                "id": format!("evidence:project:inspect-budget:item:{index:02}"),
                "supports": ["project:inspect-budget:decision:hub"],
                "text": format!("Evidence {index:02}: {}", "proof ".repeat(70)),
                "source": format!("inspect budget fixture {index:02}")
            })
        })
        .collect::<Vec<_>>();
    let seeded = call(
        &server,
        1,
        "kmp_ingest",
        json!({
            "about": "project:inspect-budget",
            "idempotency_key": "inspect-budget:seed",
            "memory": {
                "dimensions": [{"id": "inspect-budget:process", "kind": "agentic_process"}],
                "entries": [{
                    "id": "project:inspect-budget:decision:hub",
                    "kind": "decision",
                    "text": format!("Stable inspected object. {}", "core ".repeat(180)),
                    "coordinates": [{
                        "dimension": "agentic_process",
                        "scope_id": "inspect-budget:process",
                        "sequence": 1
                    }]
                }],
                "relations": [],
                "evidence": evidence
            }
        }),
    )
    .await;
    assert_eq!(seeded["memory"]["accepted"]["entries"], 1, "{seeded}");

    let mut arguments = json!({
        "about": "project:inspect-budget",
        "ref": "project:inspect-budget:decision:hub",
        "include": {"details": true, "incoming": false, "outgoing": false, "raw": false},
        "budget": {"max_bytes": 3_000}
    });
    let mut returned_evidence = Vec::new();
    let mut required_bytes = None;
    let mut pages = 0;
    loop {
        pages += 1;
        let response = server
            .handle_json_line(&tool_call(10 + pages, "kmp_inspect", arguments.clone()))
            .await
            .expect("inspect response");
        let response: Value = serde_json::from_str(&response).expect("inspect JSON");
        assert_eq!(response["result"]["isError"], false, "{response}");
        let page = &response["result"]["structuredContent"];
        assert_eq!(page["object"]["ref"], "project:inspect-budget:decision:hub");
        assert!(
            serde_json::to_string(page).expect("page serializes").len() <= 3_000,
            "{page}"
        );
        let this_required = page["page"]["required_bytes"]
            .as_u64()
            .expect("required bytes");
        assert_eq!(*required_bytes.get_or_insert(this_required), this_required);
        returned_evidence.extend(
            page["evidence"]
                .as_array()
                .expect("evidence page")
                .iter()
                .map(|item| item["id"].as_str().expect("evidence id").to_string()),
        );
        if !page["page"]["has_more"].as_bool().expect("has_more") {
            break;
        }
        arguments["page"] = json!({
            "cursor": page["page"]["next_cursor"].as_str().expect("next cursor")
        });
        assert!(pages < 20, "inspect continuation must make progress");
    }
    assert!(pages > 1, "fixture must exercise continuation");
    assert_eq!(
        returned_evidence,
        (0..18)
            .map(|index| format!("evidence:project:inspect-budget:item:{index:02}"))
            .collect::<Vec<_>>()
    );

    let floor = server
        .handle_json_line(&tool_call(
            99,
            "kmp_inspect",
            json!({
                "about": "project:inspect-budget",
                "ref": "project:inspect-budget:decision:hub",
                "include": {"details": true, "incoming": false, "outgoing": false, "raw": false},
                "budget": {"max_bytes": 512}
            }),
        ))
        .await
        .expect("floor response");
    let floor: Value = serde_json::from_str(&floor).expect("floor JSON");
    assert_eq!(floor["result"]["isError"], true, "{floor}");
    assert_eq!(
        floor["result"]["structuredContent"]["error"]["code"],
        "invalid_argument"
    );
    assert!(
        floor["result"]["structuredContent"]["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("object floor")),
        "{floor}"
    );
}

async fn call(server: &KernelMcpServer, id: u64, name: &str, arguments: Value) -> Value {
    let response = server
        .handle_json_line(&tool_call(id, name, arguments))
        .await
        .expect("tool call should produce a response");
    let value: Value = serde_json::from_str(&response).expect("response is JSON");
    assert!(
        value.get("error").is_none(),
        "tool `{name}` must not error: {value}"
    );
    value["result"]["structuredContent"].clone()
}

#[tokio::test]
async fn embedded_backend_serves_kmp_tools_and_memory_survives_sessions() {
    let data_dir = tempfile::tempdir().expect("temp data dir");

    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    let ingest = call(&server, 1, "kmp_ingest", ingest_arguments()).await;
    assert_eq!(ingest["memory"]["about"], "question:e3");
    assert_eq!(ingest["memory"]["read_after_write_ready"], true);

    let wake = call(&server, 2, "kmp_wake", json!({"about": "question:e3"})).await;
    let wake_text = wake.to_string();
    assert!(
        wake_text.contains("claim:e3"),
        "wake must surface the ingested entry: {wake_text}"
    );
    drop(server);

    let telemetry = SqliteQualityTelemetryReader::open(data_dir.path())
        .expect("quality telemetry journal opens after the session");
    let wake_observations = telemetry
        .query_since(0, Some("kmp_wake"), 10)
        .expect("wake quality observations are queryable");
    assert_eq!(wake_observations.len(), 1);
    assert_eq!(wake_observations[0].root_node_id(), "question:e3");
    drop(telemetry);

    // A brand-new session on the same data dir recovers the memory.
    let second = KernelMcpServer::embedded(data_dir.path()).expect("second session opens");
    let recovered = call(&second, 3, "kmp_wake", json!({"about": "question:e3"})).await;
    assert!(
        recovered.to_string().contains("claim:e3"),
        "second session must recover memory written by the first"
    );
}

#[tokio::test]
async fn view_intents_resolve_projection_names_against_the_mounted_store_and_reader() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(&server, 1, "kmp_ingest", ingest_arguments()).await;
    kmp_viewer::ViewRegistry::shared().set_available_overlays(vec!["causal_density".to_string()]);

    let opened = call(
        &server,
        2,
        "kmp_view_open",
        json!({"view_id": "projection-validation", "about": "question:e3"}),
    )
    .await;
    assert_eq!(opened["viewer_available"], false, "{opened}");
    assert!(
        opened["unhonored"][0]
            .as_str()
            .is_some_and(|warning| warning.contains("unavailable in this session")),
        "a semantic view without a browser must say what cannot be rendered: {opened}"
    );
    let applied = call(
        &server,
        3,
        "kmp_view_apply_intent",
        json!({
            "view_id": "projection-validation",
            "expected_revision": opened["state"]["view_revision"],
            "idempotency_key": "projection-validation-1",
            "projection": {
                "dimensions": ["conversation", "no_such_dimension"],
                "relation_classes": ["causal", "evidential"],
                "overlays": ["causal_density", "no_such_series"]
            }
        }),
    )
    .await;

    assert_eq!(
        applied["unhonored"],
        json!(["no_such_dimension", "no_such_series"]),
        "{applied}"
    );
    assert_eq!(
        applied["state"]["projection"]["dimensions"],
        json!(["conversation"])
    );
    assert_eq!(
        applied["state"]["projection"]["overlays"],
        json!(["causal_density"])
    );

    let invalid_class = call(
        &server,
        4,
        "kmp_view_apply_intent",
        json!({
            "view_id": "projection-validation",
            "idempotency_key": "projection-validation-2",
            "projection": {"relation_classes": ["telepathic"]}
        }),
    )
    .await;
    assert_eq!(invalid_class["error"]["code"], "invalid_argument");

    let backwards = call(
        &server,
        5,
        "kmp_view_apply_intent",
        json!({
            "view_id": "projection-validation",
            "idempotency_key": "projection-validation-3",
            "focus": {"time_range": {
                "axis": "observed",
                "from": "2026-08-28T00:00:00Z",
                "to": "2026-08-27T00:00:00Z"
            }}
        }),
    )
    .await;
    assert_eq!(backwards["error"]["code"], "invalid_argument");

    let nonexistent_zoom = call(
        &server,
        6,
        "kmp_view_apply_intent",
        json!({
            "view_id": "projection-validation",
            "idempotency_key": "projection-validation-4",
            "projection": {"semantic_zoom": "evidence"}
        }),
    )
    .await;
    assert_eq!(nonexistent_zoom["error"]["code"], "invalid_argument");

    let explicit_trace_window = call(
        &server,
        7,
        "kmp_view_apply_intent",
        json!({
            "view_id": "projection-validation",
            "idempotency_key": "projection-validation-5",
            "focus": {"time_range": {
                "axis": "occurred",
                "from": "2026-08-27T00:00:00Z",
                "to": "2026-08-28T00:00:00Z"
            }},
            "trace": {
                "from": "question:e3:claim:e3",
                "to": "question:e3:claim:e3-detail"
            }
        }),
    )
    .await;
    assert_eq!(
        explicit_trace_window["unhonored"],
        json!(["trace framing (explicit focus.time_range has priority)"])
    );
}

#[tokio::test]
async fn view_idempotency_replays_the_same_intent_and_refuses_a_different_one() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(&server, 1, "kmp_ingest", ingest_arguments()).await;
    let view_id = "idempotency-collision-325";
    let opened = call(
        &server,
        2,
        "kmp_view_open",
        json!({"view_id": view_id, "about": "question:e3"}),
    )
    .await;
    let selection = json!({
        "view_id": view_id,
        "expected_revision": opened["state"]["view_revision"],
        "idempotency_key": "one-intent-only",
        "selection": "question:e3:claim:e3"
    });

    let first = call(&server, 3, "kmp_view_apply_intent", selection.clone()).await;
    assert_eq!(first["applied"], true, "{first}");

    let replay = call(&server, 4, "kmp_view_apply_intent", selection).await;
    assert_eq!(replay["applied"], false, "{replay}");
    assert_eq!(replay["view_revision"], first["view_revision"]);

    let collision = call(
        &server,
        5,
        "kmp_view_apply_intent",
        json!({
            "view_id": view_id,
            "idempotency_key": "one-intent-only",
            "search": "pool saturation"
        }),
    )
    .await;
    assert_eq!(collision["error"]["code"], "conflict", "{collision}");
    assert!(
        collision["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("one-intent-only")
                && message.contains("different content")),
        "the collision must tell the caller which key it reused: {collision}"
    );

    let current = call(
        &server,
        6,
        "kmp_view_get_state",
        json!({"view_id": view_id}),
    )
    .await;
    assert_eq!(current["view_revision"], first["view_revision"]);
    assert_eq!(current["state"]["selection"], "question:e3:claim:e3");
    assert!(current["state"]["search"].is_null());
}

#[tokio::test]
async fn trace_returns_only_the_directed_path_and_warns_when_the_target_is_unreachable() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(&server, 1, "kmp_ingest", ingest_arguments()).await;

    let forward = call(
        &server,
        2,
        "kmp_trace",
        json!({
            "about": "question:e3",
            "from": "question:e3:claim:e3",
            "to": "question:e3:claim:e3-detail"
        }),
    )
    .await;
    assert_eq!(forward["trace"].as_array().map(Vec::len), Some(1));
    assert_eq!(forward["trace"][0]["from"], "question:e3:claim:e3");
    assert_eq!(forward["trace"][0]["to"], "question:e3:claim:e3-detail");

    let reverse = call(
        &server,
        3,
        "kmp_trace",
        json!({
            "about": "question:e3",
            "from": "question:e3:claim:e3-detail",
            "to": "question:e3:claim:e3"
        }),
    )
    .await;
    assert_eq!(reverse["trace"], json!([]), "{reverse}");
    assert!(
        reverse["warnings"]
            .as_array()
            .is_some_and(|warnings| !warnings.is_empty()),
        "an unreachable target must be explicit: {reverse}"
    );
}

#[tokio::test]
async fn embedded_backend_journals_quality_telemetry_for_reads() {
    use kmp_mcp::{EmbeddedKernelMcpBackend, KernelMcpToolBackend};

    let data_dir = tempfile::tempdir().expect("temp data dir");
    let backend = EmbeddedKernelMcpBackend::open(data_dir.path()).expect("backend opens");

    backend
        .call_tool("kmp_ingest", &ingest_arguments())
        .await
        .expect("ingest succeeds");
    backend
        .call_tool("kmp_wake", &serde_json::json!({"about": "question:e3"}))
        .await
        .expect("wake succeeds");
    backend
        .call_tool(
            "kmp_ask",
            &serde_json::json!({
                "about": "question:e3",
                "question": "What was accepted?"
            }),
        )
        .await
        .expect("ask succeeds");
    backend
        .call_tool(
            "kmp_trace",
            &serde_json::json!({
                "about": "question:e3",
                "from": "question:e3:claim:e3",
                "to": "question:e3:claim:e3-detail"
            }),
        )
        .await
        .expect("trace succeeds");
    backend
        .call_tool(
            "kmp_goto",
            &serde_json::json!({
                "about": "question:e3",
                "at": {"sequence": 2}
            }),
        )
        .await
        .expect("goto succeeds");
    drop(backend);

    let telemetry = SqliteQualityTelemetryReader::open(data_dir.path()).expect("journal opens");
    let wakes = telemetry
        .query_since(0, Some("kmp_wake"), 10)
        .expect("wake observations query");
    let asks = telemetry
        .query_since(0, Some("kmp_ask"), 10)
        .expect("ask observations query");
    let traces = telemetry
        .query_since(0, Some("kmp_trace"), 10)
        .expect("trace observations query");
    let gotos = telemetry
        .query_since(0, Some("kmp_goto"), 10)
        .expect("goto observations query");
    assert_eq!(wakes.len(), 1, "wake must journal one observation");
    assert_eq!(asks.len(), 1, "ask must journal one observation");
    assert_eq!(traces.len(), 1, "trace must journal one observation");
    assert_eq!(gotos.len(), 1, "goto must journal one observation");
    assert_eq!(wakes[0].root_node_id(), "question:e3");
    assert!(wakes[0].raw_equivalent_tokens() > 0);
}
