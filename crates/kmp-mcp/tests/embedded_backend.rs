//! E3 acceptance: the embedded backend serves KMP tools in-process and
//! memory survives across sessions (fresh-machine criterion analog).

use kmp_adapter_embedded::RedbQualityTelemetryReader;
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
                    "id": "claim:e3",
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
                    "id": "claim:e3-detail",
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
                "from": "claim:e3",
                "to": "claim:e3-detail",
                "rel": "supports",
                "class": "evidential",
                "why": "The accepted claim answers the checkpoint question.",
                "evidence": "E3 acceptance fixture.",
                "confidence": "high",
                "motivation": "Preserve the acceptance rationale.",
                "method": "Embedded conformance probe.",
                "decision_id": "decision:e3",
                "caused_by_node_id": "claim:e3",
                "coordinate": {
                    "dimension": "conversation",
                    "scope_id": "conversation:s1",
                    "valid_from": "2026-07-22T10:00:00Z",
                    "valid_until": "2026-07-22T10:20:00Z",
                    "sequence": 2
                }
            }],
            "evidence": [{
                "id": "evidence:e3",
                "supports": ["claim:e3"],
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

    let historical =
        kmp_mcp::snapshot::read_only(&bundle, "kmp_inspect", json!({"ref": "claim:e3"}))
            .await
            .expect("read verified snapshot in isolation");
    assert_eq!(
        historical["result"]["structuredContent"]["object"]["ref"],
        "claim:e3"
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
                "id": format!("evidence:weak:{index:03}"),
                "supports": ["claim:large-recall"],
                "text": format!(
                    "Gate deficiencies caused rejection; authority remains withheld for unrelated rollout {index}."
                ),
                "source": format!("historical gate note {index:03}")
            })
        })
        .collect::<Vec<_>>();
    evidence.push(json!({
        "id": "evidence:exact:gate-rejection",
        "supports": ["claim:large-recall"],
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
                    "id": "claim:large-recall",
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
                    "id": "claim:gate-action",
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
                "from": "claim:large-recall",
                "to": "claim:gate-action",
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
                    "id": "success:ranking-correction-merged",
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
                    "id": "constraint:restart-live-service",
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
                    "id": "success:corrected-service-installed",
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
                    "id": "error:tls-projection-race",
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
                    "from": "constraint:restart-live-service",
                    "to": "success:ranking-correction-merged",
                    "rel": "depends_on",
                    "class": "causal",
                    "why": "Live validation depends on rebuilding and restarting the service because the running executable predates the corrected relevance-ranking implementation.",
                    "evidence": "The old service reproduced the stale weak-prefix retrieval result after the corrected build passed its regression test.",
                    "confidence": "high"
                },
                {
                    "from": "success:corrected-service-installed",
                    "to": "constraint:restart-live-service",
                    "rel": "updates_state",
                    "class": "causal",
                    "why": "Installing the corrected release removes the stale executable for future launches, while an already-running process still requires restart.",
                    "evidence": "The installer replaced the old release with the corrected build; the live process was not restarted.",
                    "confidence": "high"
                },
                {
                    "from": "error:tls-projection-race",
                    "to": "constraint:restart-live-service",
                    "rel": "checked_against",
                    "class": "evidential",
                    "why": "The projection race was compared with the live service restart constraint while triaging issue 80.",
                    "evidence": "The comparison mentioned the retrieval regression, required rebuild and restart, and later validation against the live service.",
                    "confidence": "high"
                }
            ],
            "evidence": [
                {
                    "id": "evidence:old-live-service",
                    "supports": ["constraint:restart-live-service"],
                    "text": "The running service used an older executable while the corrected repository build was newer. Its live query reproduced the stale weak-prefix result, so the corrected implementation had not yet been validated in that process.",
                    "source": "live validation probe"
                },
                {
                    "id": "evidence:corrected-install",
                    "supports": ["success:corrected-service-installed"],
                    "text": "The installer replaced the previous release with the corrected build for subsequent service launches.",
                    "source": "installation verification"
                },
                {
                    "id": "evidence:tls-projection-race",
                    "supports": ["error:tls-projection-race"],
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
                    "id": "decision:sqlite-wal",
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
                    "id": "constraint:shared-process-store",
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
                    "id": "outcome:migration-replay",
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
                    "id": "note:earlier-features",
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
                    "id": "note:format-layout",
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
                    "id": "note:release-verification",
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
                    "from": "decision:sqlite-wal",
                    "to": "constraint:shared-process-store",
                    "rel": "chosen_because",
                    "class": "motivational",
                    "why": "SQLite WAL replaced redb because concurrent processes need one shareable embedded store.",
                    "evidence": "The multi-process architecture decision records the engine comparison and concurrency requirement.",
                    "confidence": "high"
                },
                {
                    "from": "outcome:migration-replay",
                    "to": "decision:sqlite-wal",
                    "rel": "depends_on",
                    "class": "causal",
                    "why": "Replay preserves legacy redb data while the SQLite decision governs fresh shared stores.",
                    "evidence": "Migration compatibility and the current engine decision were verified together.",
                    "confidence": "high"
                }
            ],
            "evidence": [
                {
                    "id": "evidence:sqlite-wal-decision",
                    "supports": ["decision:sqlite-wal"],
                    "text": "SQLite WAL is the embedded storage engine selected for concurrent KMP processes sharing one store.",
                    "source": "ADR fixture"
                },
                {
                    "id": "evidence:shared-process-constraint",
                    "supports": ["constraint:shared-process-store"],
                    "text": "Two KMP processes require a shareable embedded store.",
                    "source": "concurrency fixture"
                },
                {
                    "id": "evidence:migration-replay",
                    "supports": ["outcome:migration-replay"],
                    "text": "Existing redb stores remain readable during migration; new stores select SQLite.",
                    "source": "migration fixture"
                },
                {
                    "id": "evidence:earlier-features",
                    "supports": ["note:earlier-features"],
                    "text": "KMP added more features earlier than one roadmap expected.",
                    "source": "weak lexical distractor"
                },
                {
                    "id": "evidence:format-layout",
                    "supports": ["note:format-layout"],
                    "text": "KMP version names use the same format layout.",
                    "source": "weak lexical distractor"
                },
                {
                    "id": "evidence:release-verification",
                    "supports": ["note:release-verification"],
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
                    "id": "decision:two-engine-architecture",
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
                    "id": "decision:historical-fresh-store-default",
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
                    "id": "decision:current-fresh-store-default",
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
                    "from": "decision:current-fresh-store-default",
                    "to": "decision:historical-fresh-store-default",
                    "rel": "updates_state",
                    "class": "causal",
                    "why": "The shipped fresh-store policy changes only the distribution default, not the two-engine architecture.",
                    "evidence": "A fresh data directory now selects SQLite while existing redb stores remain readable.",
                    "confidence": "high"
                },
                {
                    "from": "decision:current-fresh-store-default",
                    "to": "decision:two-engine-architecture",
                    "rel": "uses_background",
                    "class": "evidential",
                    "why": "The new default operates inside the existing two-engine compatibility architecture.",
                    "evidence": "Existing redb stores remain readable while fresh stores select SQLite.",
                    "confidence": "high"
                }
            ],
            "evidence": [
                {
                    "id": "evidence:historical-fresh-store-default",
                    "supports": ["decision:historical-fresh-store-default"],
                    "text": "Before the distribution change, fresh KMP data directories defaulted to redb.",
                    "source": "historical release fixture",
                    "time": "2026-08-17T11:38:00Z"
                },
                {
                    "id": "evidence:current-fresh-store-default",
                    "supports": ["decision:current-fresh-store-default"],
                    "text": "Existing redb stores remain readable; a new KMP installation creates a fresh SQLite store by default.",
                    "source": "current release fixture",
                    "time": "2026-08-17T11:39:00Z"
                },
                {
                    "id": "evidence:two-engine-architecture",
                    "supports": ["decision:two-engine-architecture"],
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
                "id": "constraint:share-embedded-store",
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
                "id": "evidence:shared-store-requirement",
                "supports": ["constraint:share-embedded-store"],
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
                    "id": "decision:embedded-redb",
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
                    "id": "constraint:single-writer-ownership",
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
                "from": "decision:embedded-redb",
                "to": "constraint:single-writer-ownership",
                "rel": "uses_background",
                "class": "evidential",
                "why": "The single-writer model matches the product's per-project agent ownership.",
                "evidence": "ADR-011 records one writer per redb store.",
                "confidence": "high"
            }],
            "evidence": [{
                "id": "evidence:embedded-redb-choice",
                "supports": ["decision:embedded-redb"],
                "text": "We chose redb because one writer matched one agent per project.",
                "source": "archive/docs/adr/ADR-011.md:42",
                "metadata": {"language": "en", "digest": "sha256:language-fixture"}
            }]
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
        .find(|evidence| evidence["id"] == "detail:evidence:embedded-redb-choice")
        .expect("English retry cites the stored evidence");
    assert_eq!(evidence["text"], TEXT);
    assert_eq!(evidence["source"], "archive/docs/adr/ADR-011.md:42");
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
    assert!(
        goto["quality"]["causal_density"]
            .as_f64()
            .expect("goto quality has causal density")
            > 0.0,
        "evidential and motivational relations count as explanatory"
    );
    let entry = goto["entries"]
        .as_array()
        .expect("goto entries are an array")
        .iter()
        .find(|entry| entry["ref"] == "claim:e3")
        .expect("goto returns the first claim");
    assert_eq!(entry["metadata"]["window"], "10:00-10:20");
    assert_eq!(entry["metadata"]["probe_digest"], "sha256:e3");

    let evidence = goto["proof"]["evidence"]
        .as_array()
        .expect("goto proof evidence is an array")
        .iter()
        .find(|evidence| evidence["id"] == "detail:evidence:e3")
        .expect("goto returns explicit evidence");
    assert_eq!(evidence["source"], "embedded backend test");
    assert_eq!(evidence["supports"], json!(["claim:e3"]));
    assert_eq!(evidence["metadata"]["requested_by"], "choreographer");

    let relation = goto["proof"]["path"]
        .as_array()
        .expect("goto proof path is an array")
        .iter()
        .find(|relation| relation["from"] == "claim:e3" && relation["to"] == "claim:e3-detail")
        .expect("goto returns the qualified relation");
    assert_eq!(relation["motivation"], "Preserve the acceptance rationale.");
    assert_eq!(relation["method"], "Embedded conformance probe.");
    assert_eq!(relation["decision_id"], "decision:e3");
    assert_eq!(relation["caused_by_node_id"], "claim:e3");
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
        .find(|evidence| evidence["id"] == "detail:evidence:e3")
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
        .find(|evidence| evidence["id"] == "detail:evidence:e3")
        .expect("ask returns explicit evidence");
    assert_eq!(ask_evidence["source"], "embedded backend test");

    let inspected_entry = call(&server, 3, "kmp_inspect", json!({"ref": "claim:e3"})).await;
    assert_eq!(
        inspected_entry["object"]["metadata"]["window"],
        "10:00-10:20"
    );

    let inspected_evidence = call(&server, 4, "kmp_inspect", json!({"ref": "evidence:e3"})).await;
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
    assert!(
        wake["wake"]["current_state"][0]
            .as_str()
            .is_some_and(|state| state.contains("--triggers-->")),
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
            evidence_ids.contains(&"detail:evidence:sqlite-wal-decision"),
            "the answer-bearing architecture decision must survive reranking: {ask}"
        );
        assert!(
            ask["because"]
                .as_array()
                .expect("answer citations")
                .iter()
                .any(|reason| reason["claim"] == "decision:sqlite-wal"),
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
                ask["proof"]["evidence"][0]["supports"][0], "decision:current-fresh-store-default",
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
    assert!(before_refs.contains(&"decision:historical-fresh-store-default"));
    assert!(!before_refs.contains(&"decision:current-fresh-store-default"));

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
            .any(|entry| entry["ref"] == "decision:current-fresh-store-default"),
        "{after}"
    );

    let architecture = call(
        &server,
        10,
        "kmp_inspect",
        json!({
            "ref": "decision:two-engine-architecture",
            "include": {"incoming": true, "outgoing": true}
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
        json!({"ref": "constraint:share-embedded-store"}),
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
                "ref": "decision:sqlite-wal-shared-store",
                "kind": "decision",
                "summary": "Use SQLite WAL instead of redb for shared embedded storage.",
                "evidence": "The architecture comparison selected SQLite WAL over redb after exercising two independent processes."
            },
            "connect_to": [{
                "ref": "constraint:share-embedded-store",
                "rel": "chosen_because",
                "class": "motivational",
                "why": WHY,
                "evidence": RELATION_EVIDENCE,
                "confidence": "high"
            }],
            "read_context": {
                "inspected_refs": ["constraint:share-embedded-store"]
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
        ask["because"].as_array().is_some_and(|reasons| reasons
            .iter()
            .any(|reason| { reason["claim"] == "decision:sqlite-wal-shared-store" })),
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
            "from": "decision:sqlite-wal-shared-store",
            "to": "constraint:share-embedded-store"
        }),
    )
    .await;
    assert!(trace.to_string().contains(WHY), "{trace}");
    assert!(trace.to_string().contains(RELATION_EVIDENCE), "{trace}");

    let relation_proof = call(
        &server,
        8,
        "kmp_inspect",
        json!({"ref": "evidence:decision:sqlite-wal-shared-store:relation:1"}),
    )
    .await;
    assert!(
        relation_proof.to_string().contains(RELATION_EVIDENCE),
        "{relation_proof}"
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

    let telemetry = RedbQualityTelemetryReader::open(data_dir.path())
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
                "from": "claim:e3",
                "to": "claim:e3-detail"
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

    let telemetry = RedbQualityTelemetryReader::open(data_dir.path()).expect("journal opens");
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
