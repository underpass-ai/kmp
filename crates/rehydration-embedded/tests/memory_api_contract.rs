//! The embedded kernel behind the published consumer contract.
//!
//! Everything here goes through [`MemoryRecallApi`] and the plain types of
//! `rehydration-memory-api` — the way an embedding product sees the kernel. If
//! a test in this file needs a domain type to *assert* something, the contract
//! is leaking; domain types appear only to arrange the scene.

use rehydration_application::{
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryIngestCommand,
};
use rehydration_embedded::EmbeddedKernel;
use rehydration_memory_api::{
    CONTRACT_VERSION, MemoryAnswerPolicy, MemoryAskRequest, MemoryCoordinateSpec,
    MemoryDimensionSpec, MemoryEntrySpec, MemoryEvidenceSpec, MemoryProvenanceSpec,
    MemoryRecallApi, MemoryRecordApi, MemoryRecordRequest, MemoryRelationSpec, MemoryWakeRequest,
};

const ABOUT: &str = "project:memory-api";

fn corpus() -> MemoryIngestCommand {
    MemoryIngestCommand {
        about: ABOUT.to_string(),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: "timeline:work".to_string(),
                kind: "timeline".to_string(),
                title: None,
                metadata: Default::default(),
            }],
            entries: vec![MemoryEntryData {
                id: "decision:first".to_string(),
                kind: "decision".to_string(),
                text: "The first decision.".to_string(),
                coordinates: vec![MemoryCoordinateData {
                    dimension: "timeline".to_string(),
                    scope_id: "timeline:work".to_string(),
                    occurred_at: Some("2026-08-01T10:00:00Z".to_string()),
                    observed_at: None,
                    ingested_at: None,
                    valid_from: None,
                    valid_until: None,
                    sequence: Some(1),
                    rank: None,
                    metadata: Default::default(),
                }],
                metadata: Default::default(),
            }],
            relations: vec![],
            evidence: vec![],
        },
        provenance: None,
        idempotency_key: "memory-api-test".to_string(),
        dry_run: false,
    }
}

async fn kernel_with_memory() -> (tempfile::TempDir, EmbeddedKernel) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");
    kernel
        .service()
        .ingest(corpus())
        .await
        .expect("the corpus ingests");
    (directory, kernel)
}

fn wake_request() -> MemoryWakeRequest {
    MemoryWakeRequest {
        about: ABOUT.to_string(),
        role: "consumer-contract-test".to_string(),
        intent: "prove the published contract".to_string(),
        dimension_kinds: Vec::new(),
        scoped_to_about: false,
        token_budget: 4096,
        depth: 2,
        max_tier: None,
        max_entries: None,
    }
}

#[tokio::test]
async fn the_report_names_the_contract_the_release_and_every_method() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");

    let report = MemoryRecallApi::capabilities(&kernel);
    assert_eq!(report.contract_version(), CONTRACT_VERSION);
    assert!(!report.library_version().is_empty());
    assert!(report.supports("wake"));
    assert!(
        report.supports("ask"),
        "a method that exists but is not declared cannot be checked at startup"
    );
    assert!(report.supports("record"));
    assert_eq!(
        MemoryRecordApi::capabilities(&kernel),
        report,
        "one implementation gives one account of itself, whichever trait asks"
    );
}

#[tokio::test]
async fn a_consumer_recalls_memory_without_a_domain_type_in_sight() {
    let (_directory, kernel) = kernel_with_memory().await;

    let recall = kernel.wake(wake_request()).await.expect("the wake answers");

    assert_eq!(recall.about, ABOUT, "the about is echoed back unchanged");
    assert!(
        recall
            .neighbors
            .iter()
            .any(|node| node.node_id == "decision:first"),
        "the ingested decision must come back as a plain node: {:?}",
        recall.neighbors
    );
    assert!(
        !recall.rendered.content.is_empty(),
        "a recall carries the rendered context a reader consumes"
    );
    assert!(
        !recall.rendered.content_hash.is_empty(),
        "the hash is what lets a consumer verify the model received exactly \
         what the kernel rendered"
    );
}

#[tokio::test]
async fn a_question_is_answered_under_the_strict_policy() {
    let (_directory, kernel) = kernel_with_memory().await;

    let answer = kernel
        .ask(MemoryAskRequest {
            about: ABOUT.to_string(),
            question: "What was decided first?".to_string(),
            answer_policy: MemoryAnswerPolicy::EvidenceOrUnknown,
            dimension_kinds: Vec::new(),
            scoped_to_about: false,
            token_budget: 4096,
            depth: 2,
            max_tier: None,
        })
        .await
        .expect("the ask answers");

    assert_eq!(answer.about, ABOUT);
    assert!(!answer.rendered.content.is_empty());
}

#[tokio::test]
async fn an_unknown_about_does_not_read_as_a_healthy_empty_memory() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");

    let outcome = kernel
        .wake(MemoryWakeRequest {
            about: "project:nowhere".to_string(),
            ..wake_request()
        })
        .await;

    match outcome {
        Err(error) => assert!(
            !error.is_transient(),
            "an about that does not exist will not appear by retrying: {error}"
        ),
        Ok(recall) => assert!(
            recall.neighbors.is_empty() && recall.details.is_empty(),
            "an unknown about must answer empty or not-found, never someone \
             else's memory: {recall:?}"
        ),
    }
}

fn record_request(key: &str, text: &str) -> MemoryRecordRequest {
    MemoryRecordRequest {
        about: "project:recorded".to_string(),
        dimensions: vec![MemoryDimensionSpec {
            id: "timeline:observed".to_string(),
            kind: "timeline".to_string(),
            title: None,
            metadata: Default::default(),
        }],
        entries: vec![MemoryEntrySpec {
            id: "observation:first".to_string(),
            kind: "observation".to_string(),
            text: text.to_string(),
            coordinates: vec![MemoryCoordinateSpec {
                dimension: "timeline".to_string(),
                scope_id: "timeline:observed".to_string(),
                occurred_at: Some("2026-08-04T10:00:00Z".to_string()),
                sequence: Some(1),
                rank: None,
            }],
            metadata: Default::default(),
        }],
        relations: Vec::new(),
        evidence: vec![MemoryEvidenceSpec {
            id: "evidence:first".to_string(),
            supports: vec!["observation:first".to_string()],
            text: "A synthetic reading backing the observation.".to_string(),
            source: Some("contract-test".to_string()),
            time: Some("2026-08-04T10:00:00Z".to_string()),
            metadata: Default::default(),
        }],
        provenance: Some(MemoryProvenanceSpec {
            source_kind: "projection".to_string(),
            source_agent: "memory-api-contract-test".to_string(),
            observed_at: "2026-08-04T10:00:05Z".to_string(),
            correlation_id: Some("corr-record-1".to_string()),
            causation_id: None,
        }),
        idempotency_key: key.to_string(),
    }
}

#[tokio::test]
async fn what_a_consumer_records_a_consumer_recalls() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");

    let recorded = kernel
        .record(record_request("record:contract:1", "The retries began."))
        .await
        .expect("the record lands");
    assert_eq!(recorded.about, "project:recorded");
    assert_eq!(recorded.accepted_entries, 1);
    assert_eq!(recorded.accepted_evidence, 1);
    assert!(!recorded.memory_id.is_empty());

    let recall = kernel
        .wake(MemoryWakeRequest {
            about: "project:recorded".to_string(),
            ..wake_request()
        })
        .await
        .expect("the wake answers");
    assert!(
        recall
            .neighbors
            .iter()
            .any(|node| node.node_id == "observation:first"),
        "what went in through the contract must come back through the \
         contract: {:?}",
        recall.neighbors
    );
}

#[tokio::test]
async fn a_retried_record_answers_with_the_same_outcome_not_a_second_memory() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");

    let first = kernel
        .record(record_request(
            "record:contract:retry",
            "The retries began.",
        ))
        .await
        .expect("the first record lands");
    let second = kernel
        .record(record_request(
            "record:contract:retry",
            "The retries began.",
        ))
        .await
        .expect("the retry is an answer, not an error");

    assert_eq!(
        second.memory_id, first.memory_id,
        "an at-least-once pipeline must be able to apply twice and store once"
    );
}

#[tokio::test]
async fn a_reused_key_with_different_content_is_refused_not_retried() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");

    kernel
        .record(record_request(
            "record:contract:reuse",
            "The retries began.",
        ))
        .await
        .expect("the first record lands");
    let error = kernel
        .record(record_request(
            "record:contract:reuse",
            "Something else entirely.",
        ))
        .await
        .expect_err("the same key must not quietly mean two different things");

    assert!(
        !error.is_transient(),
        "a consumer told to wait would retry a contradiction forever: {error}"
    );
}

#[tokio::test]
async fn a_record_without_an_idempotency_key_is_refused() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");

    let error = kernel
        .record(record_request("", "The retries began."))
        .await
        .expect_err("an unkeyed record cannot be replayed safely");
    assert!(!error.is_transient());
}

#[tokio::test]
async fn a_recorded_relation_comes_back_as_a_relationship() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");

    let mut request = record_request("record:contract:related", "The retries began.");
    let mut second = request.entries[0].clone();
    second.id = "observation:second".to_string();
    second.text = "The deploy finished moments earlier.".to_string();
    request.entries.push(second);
    // Relations join entries — evidence declared alongside resolves later and
    // cannot be an endpoint, which the spec's doc says out loud.
    request.relations = vec![MemoryRelationSpec {
        from: "observation:second".to_string(),
        to: "observation:first".to_string(),
        rel: "supports".to_string(),
        semantic_class: "evidential".to_string(),
        why: Some("the deploy timing explains the retries".to_string()),
        confidence: Some("high".to_string()),
        sequence: None,
    }];
    let recorded = kernel.record(request).await.expect("the record lands");
    assert_eq!(recorded.accepted_relations, 1);

    let recall = kernel
        .wake(MemoryWakeRequest {
            about: "project:recorded".to_string(),
            ..wake_request()
        })
        .await
        .expect("the wake answers");
    assert!(
        recall.relationships.iter().any(|relationship| relationship
            .source_node_id
            .contains("observation:second")
            && relationship.target_node_id.contains("observation:first")),
        "the recorded link must come back through the contract: {:?}",
        recall.relationships
    );
}

#[tokio::test]
async fn a_claimed_link_without_a_stated_reason_is_refused() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");

    let mut request = record_request("record:contract:unreasoned", "The retries began.");
    let mut second = request.entries[0].clone();
    second.id = "observation:second".to_string();
    request.entries.push(second);
    request.relations = vec![MemoryRelationSpec {
        from: "observation:second".to_string(),
        to: "observation:first".to_string(),
        rel: "supports".to_string(),
        semantic_class: "evidential".to_string(),
        why: None,
        confidence: None,
        sequence: None,
    }];
    let error = kernel
        .record(request)
        .await
        .expect_err("an evidential link with no reason and no confidence must not land");
    assert!(!error.is_transient());
}

#[tokio::test]
async fn a_zero_rank_is_refused_by_the_kernels_own_validation() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");

    let mut request = record_request("record:contract:ranked", "The retries began.");
    request.entries[0].coordinates[0].rank = Some(0);
    let error = kernel
        .record(request)
        .await
        .expect_err("rank is one-based in the kernel, and the contract inherits that");
    assert!(!error.is_transient());
}

#[tokio::test]
async fn a_recall_names_the_snapshot_it_answered_from() {
    let (_directory, kernel) = kernel_with_memory().await;

    let first = kernel.wake(wake_request()).await.expect("the wake answers");
    let second = kernel.wake(wake_request()).await.expect("the wake answers");

    assert!(first.revision > 0, "a served recall has a real revision");
    assert!(!first.content_hash.is_empty());
    assert_eq!(
        (first.revision, &first.content_hash),
        (second.revision, &second.content_hash),
        "an untouched memory answers twice from one snapshot — which is the \
         check a consumer combining recalls performs instead of hoping"
    );
    assert_ne!(
        first.content_hash, first.rendered.content_hash,
        "the snapshot's identity and the rendered text's hash are different \
         facts; a contract that reused one for the other would let a consumer \
         verify the wrong thing"
    );
}

#[tokio::test]
async fn a_recall_accounts_for_the_quality_of_its_rendering() {
    let (_directory, kernel) = kernel_with_memory().await;

    let recall = kernel.wake(wake_request()).await.expect("the wake answers");
    let quality = &recall.rendered.quality;
    assert!(
        quality.raw_equivalent_tokens > 0,
        "a rendering of a non-empty memory stands in for something: {quality:?}"
    );
    for (name, value) in [
        ("compression_ratio", quality.compression_ratio),
        ("causal_density", quality.causal_density),
        ("noise_ratio", quality.noise_ratio),
        ("detail_coverage", quality.detail_coverage),
    ] {
        assert!(value.is_finite(), "{name} must be a number: {quality:?}");
        assert!(value >= 0.0, "{name} must not be negative: {quality:?}");
    }
}

#[tokio::test]
async fn the_kernel_accounts_for_contract_recalls_in_its_own_telemetry() {
    let directory = tempfile::tempdir().expect("temporary directory");
    {
        let kernel = EmbeddedKernel::open(directory.path()).expect("the kernel opens");
        kernel
            .service()
            .ingest(corpus())
            .await
            .expect("the corpus ingests");
        kernel.wake(wake_request()).await.expect("the wake answers");
        // Dropping the kernel flushes its telemetry guard.
    }

    let reader = rehydration_adapter_embedded::RedbQualityTelemetryReader::open(directory.path())
        .expect("the telemetry store opens");
    let observed = reader.latest(10).expect("the telemetry reads");
    assert!(
        observed
            .iter()
            .any(|observation| observation.rpc() == "kernel_wake"),
        "a recall served through the contract must appear in the kernel's own \
         quality telemetry — no consumer remembered anything to make it so: \
         {observed:?}"
    );
}
