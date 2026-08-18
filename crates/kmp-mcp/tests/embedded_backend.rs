//! E3 acceptance: the embedded backend serves KMP tools in-process and
//! memory survives across sessions (fresh-machine criterion analog).

use kmp_adapter_embedded::RedbQualityTelemetryReader;
use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_domain::TokenEstimator;
use kmp_mcp::KernelMcpServer;
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

#[tokio::test]
async fn embedded_backend_round_trips_entry_metadata_and_evidence_source() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    call(&server, 1, "kernel_ingest", ingest_arguments()).await;

    let goto = call(
        &server,
        2,
        "kernel_goto",
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

    let wake = call(&server, 5, "kernel_wake", json!({"about": "question:e3"})).await;
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
        "kernel_ask",
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

    let inspected_entry = call(&server, 3, "kernel_inspect", json!({"ref": "claim:e3"})).await;
    assert_eq!(
        inspected_entry["object"]["metadata"]["window"],
        "10:00-10:20"
    );

    let inspected_evidence =
        call(&server, 4, "kernel_inspect", json!({"ref": "evidence:e3"})).await;
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
    call(&server, 1, "kernel_ingest", large_recall_ingest_arguments()).await;
    let budget = json!({
        "tokens": TOKEN_LIMIT,
        "detail": "balanced",
        "max_entries": 12
    });

    let ask = call(
        &server,
        2,
        "kernel_ask",
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
        "kernel_wake",
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
        "kernel_ingest",
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
                "kernel_ask",
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
        "kernel_ask",
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
async fn embedded_backend_returns_structured_not_found_errors() {
    let data_dir = tempfile::tempdir().expect("temp data dir");
    let server = KernelMcpServer::embedded(data_dir.path()).expect("embedded server opens");
    let error = call(
        &server,
        1,
        "kernel_goto",
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
    let ingest = call(&server, 1, "kernel_ingest", ingest_arguments()).await;
    assert_eq!(ingest["memory"]["about"], "question:e3");
    assert_eq!(ingest["memory"]["read_after_write_ready"], true);

    let wake = call(&server, 2, "kernel_wake", json!({"about": "question:e3"})).await;
    let wake_text = wake.to_string();
    assert!(
        wake_text.contains("claim:e3"),
        "wake must surface the ingested entry: {wake_text}"
    );
    drop(server);

    let telemetry = RedbQualityTelemetryReader::open(data_dir.path())
        .expect("quality telemetry journal opens after the session");
    let wake_observations = telemetry
        .query_since(0, Some("kernel_wake"), 10)
        .expect("wake quality observations are queryable");
    assert_eq!(wake_observations.len(), 1);
    assert_eq!(wake_observations[0].root_node_id(), "question:e3");
    drop(telemetry);

    // A brand-new session on the same data dir recovers the memory.
    let second = KernelMcpServer::embedded(data_dir.path()).expect("second session opens");
    let recovered = call(&second, 3, "kernel_wake", json!({"about": "question:e3"})).await;
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
        .call_tool("kernel_ingest", &ingest_arguments())
        .await
        .expect("ingest succeeds");
    backend
        .call_tool("kernel_wake", &serde_json::json!({"about": "question:e3"}))
        .await
        .expect("wake succeeds");
    backend
        .call_tool(
            "kernel_ask",
            &serde_json::json!({
                "about": "question:e3",
                "question": "What was accepted?"
            }),
        )
        .await
        .expect("ask succeeds");
    backend
        .call_tool(
            "kernel_trace",
            &serde_json::json!({
                "from": "claim:e3",
                "to": "claim:e3-detail"
            }),
        )
        .await
        .expect("trace succeeds");
    backend
        .call_tool(
            "kernel_goto",
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
        .query_since(0, Some("kernel_wake"), 10)
        .expect("wake observations query");
    let asks = telemetry
        .query_since(0, Some("kernel_ask"), 10)
        .expect("ask observations query");
    let traces = telemetry
        .query_since(0, Some("kernel_trace"), 10)
        .expect("trace observations query");
    let gotos = telemetry
        .query_since(0, Some("kernel_goto"), 10)
        .expect("goto observations query");
    assert_eq!(wakes.len(), 1, "wake must journal one observation");
    assert_eq!(asks.len(), 1, "ask must journal one observation");
    assert_eq!(traces.len(), 1, "trace must journal one observation");
    assert_eq!(gotos.len(), 1, "goto must journal one observation");
    assert_eq!(wakes[0].root_node_id(), "question:e3");
    assert!(wakes[0].raw_equivalent_tokens() > 0);
}
