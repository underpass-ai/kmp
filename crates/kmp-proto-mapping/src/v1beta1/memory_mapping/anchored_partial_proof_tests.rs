//! How much proof a gated answer carries (26 Sept 2026): a PARTIAL keeps the
//! relations that audit what it cites and the lifecycle of the rest, and the
//! gate names each missing thing once. Without the gate nothing changes.
use kmp_application::{GetContextResult, MemoryAnswerPolicy};
use kmp_domain::TemporalSelection;
use kmp_proto::v1beta1::{AnswerStatus, AskResponse, MemoryRelation};

use super::anchored_content_tests::store_with_properties;
use super::anchored_gate_tests::{ask_in, cited, missing, status, store_related};
use super::{AskGate, AskRetrievalContext, LexicalBridge, ask_response_from_result};

const QUESTION: &str = "What rollback plan and retry policy are recorded for C6.4?";
/// A question the ungated rule answers from the same store.
const PLAIN: &str = "What retry policy is pending for the C6.4 local execution adapter?";

const ENTRIES: &[(&str, &str, &str)] = &[
    (
        "decision:c64-adapter",
        "decision",
        "C6.4 local execution adapter retry policy runs ceremony steps inside the operator sandbox.",
    ),
    (
        "observation:c64-pending",
        "observation",
        "C6.4 pending work: the retry policy of the local execution adapter is not implemented yet.",
    ),
    (
        "decision:c64-retry-limit",
        "decision",
        "C6.4 retry policy limit: the local execution adapter retries three times.",
    ),
    (
        "decision:c64-retry-backoff",
        "decision",
        "C6.4 retry policy backoff: the local execution adapter waits ten seconds.",
    ),
    (
        "observation:c64-retry-owner",
        "observation",
        "C6.4 retry policy owner: the local execution adapter team reviews it.",
    ),
    (
        "observation:c64-note",
        "observation",
        "C6.4 note: the owners met on Monday.",
    ),
    (
        "observation:c64-note-v2",
        "observation",
        "C6.4 note: the owners met on Tuesday.",
    ),
    (
        "observation:pool-retry",
        "observation",
        "The worker pool retry policy allows five attempts per queued ceremony.",
    ),
    (
        "decision:c65-scheduler",
        "decision",
        "C6.5 remote scheduler dispatches queued ceremonies to the worker pool.",
    ),
    (
        "observation:billing-cluster",
        "observation",
        "The Kubernetes cluster for the billing service was upgraded to version 1.30.",
    ),
];

const RELATED: &[(&str, &str, &str)] = &[
    ("observation:c64-pending", "follows", "decision:c64-adapter"),
    (
        "observation:c64-note-v2",
        "follows",
        "decision:c65-scheduler",
    ),
    (
        "observation:c64-note-v2",
        "supersedes",
        "observation:c64-note",
    ),
    (
        "observation:pool-retry",
        "follows",
        "decision:c65-scheduler",
    ),
];

fn has(path: &[MemoryRelation], source: &str, rel: &str, target: &str) -> bool {
    path.iter().any(|relation| {
        relation.source_ref == source && relation.rel == rel && relation.target_ref == target
    })
}

fn path(response: &AskResponse) -> &[MemoryRelation] {
    &response.proof.as_ref().expect("proof").path
}

fn evidence_ids(response: &AskResponse) -> Vec<&str> {
    response
        .proof
        .as_ref()
        .expect("proof")
        .evidence
        .iter()
        .map(|item| item.id.as_str())
        .collect()
}

#[test]
fn a_partial_keeps_the_path_of_what_it_cites_and_the_lifecycle_of_the_rest() {
    let partial = ask_in(
        Some(AskGate::anchored(true)),
        QUESTION,
        store_related(ENTRIES, RELATED),
    );
    assert_eq!(status(&partial), AnswerStatus::Partial);
    let claims = cited(&partial);
    assert!(
        claims.iter().all(|claim| claim.contains("c64")),
        "{claims:?}"
    );
    assert!(!claims.contains(&"observation:c64-note-v2"), "{claims:?}");
    // The uncited memory about the anchor is still proof, but its routes are
    // not; its supersession is.
    assert!(
        evidence_ids(&partial).contains(&"entry:observation:c64-note-v2"),
        "{:?}",
        evidence_ids(&partial)
    );
    let path = path(&partial);
    assert!(has(
        path,
        "observation:c64-pending",
        "follows",
        "decision:c64-adapter"
    ));
    assert!(has(
        path,
        "timeline:main",
        "contains_entry",
        "observation:c64-pending"
    ));
    assert!(has(
        path,
        "observation:c64-note-v2",
        "supersedes",
        "observation:c64-note"
    ));
    assert!(!has(
        path,
        "observation:c64-note-v2",
        "follows",
        "decision:c65-scheduler"
    ));
    assert!(!has(
        path,
        "timeline:main",
        "contains_entry",
        "observation:c64-note-v2"
    ));
    assert!(
        partial
            .proof
            .as_ref()
            .expect("proof")
            .superseded
            .iter()
            .any(|item| item.r#ref == "observation:c64-note")
    );
}

#[test]
fn a_partial_or_unknown_proof_holds_only_memories_about_the_anchor() {
    let store = || store_related(ENTRIES, RELATED);
    let partial = ask_in(Some(AskGate::anchored(true)), QUESTION, store());
    assert_eq!(status(&partial), AnswerStatus::Partial);
    let unknown = ask_in(Some(AskGate::anchored(false)), QUESTION, store());
    assert_eq!(status(&unknown), AnswerStatus::Unknown);
    for response in [&partial, &unknown] {
        let ids = evidence_ids(response);
        assert!(!ids.is_empty());
        assert!(ids.iter().all(|id| id.contains("c64")), "{ids:?}");
    }
    // An answered reading keeps its whole proof: the worker pool's retry
    // policy shares the question's words and is still there.
    let answered = ask_in(
        Some(AskGate::anchored(true)),
        "Which retry policy decisions exist for C6.4?",
        store(),
    );
    assert_eq!(status(&answered), AnswerStatus::Answered);
    assert!(
        evidence_ids(&answered).contains(&"entry:observation:pool-retry"),
        "{:?}",
        evidence_ids(&answered)
    );
}

#[test]
fn an_answered_gate_and_no_gate_keep_the_whole_path() {
    let answered = ask_in(
        Some(AskGate::anchored(true)),
        "Which decisions and pending work exist for C6.4?",
        store_related(ENTRIES, RELATED),
    );
    assert_eq!(status(&answered), AnswerStatus::Answered);
    let plain = ask_in(None, PLAIN, store_related(ENTRIES, RELATED));
    for response in [&answered, &plain] {
        let cited_refs = response
            .because
            .iter()
            .map(|reason| reason.claim.as_str())
            .collect::<Vec<_>>();
        assert!(!cited_refs.is_empty());
        // Every relation incident to a proof entry, cited or not.
        let proof_refs = response
            .proof
            .as_ref()
            .expect("proof")
            .evidence
            .iter()
            .flat_map(|item| item.supports.iter().map(String::as_str))
            .collect::<Vec<_>>();
        for (source, rel, target) in RELATED {
            if proof_refs.contains(source) || proof_refs.contains(target) {
                assert!(
                    has(path(response), source, rel, target),
                    "{source} {rel} {target}"
                );
            }
        }
    }
}

fn store_with_one_writer() -> GetContextResult {
    let writer = "kmp_write_memory:traveler-1";
    let more = [
        (
            "observation:c64-retry-note",
            "observation",
            "C6.4 retry policy note: the local execution adapter retries a failed step once.",
        ),
        (
            "decision:c64-retry-owner",
            "decision",
            "C6.4 retry policy owner: the local execution adapter team reviews every change.",
        ),
    ];
    let entries = ENTRIES
        .iter()
        .chain(&more)
        .map(|(id, kind, text)| (*id, *kind, *text, writer, ""))
        .collect::<Vec<_>>();
    store_with_properties(&entries)
}

fn ask_capped(gate: Option<AskGate>, question: &str) -> AskResponse {
    let context = AskRetrievalContext::from(store_with_one_writer());
    let context = match gate {
        Some(gate) => context.with_gate(gate),
        None => context,
    };
    ask_response_from_result(
        question,
        None,
        MemoryAnswerPolicy::EvidenceOrUnknown,
        Some(1),
        context,
        &LexicalBridge::none(),
        &TemporalSelection::Frontier,
    )
    .expect("an ask response")
}

#[test]
fn a_partial_names_only_what_the_question_asked_and_did_not_find() {
    let gated = ask_capped(Some(AskGate::anchored(true)), QUESTION);
    assert_eq!(status(&gated), AnswerStatus::Partial);
    // More than one entry of the writer was withheld by `max_entries`...
    let omitted = gated
        .projection
        .as_ref()
        .map_or(0, |projection| projection.selection_omitted);
    assert!(omitted > 1, "{omitted}");
    // ...and none of their sources reads as something the question lacked:
    // `missing` is the reader's words and nothing else.
    let names = missing(&gated);
    assert_eq!(names, ["rollback", "plan"]);
    let proof = gated.proof.as_ref().expect("proof");
    assert_eq!(proof.frontier_size as usize, names.len());
}

#[test]
fn without_the_gate_the_withheld_sources_are_listed_as_before() {
    let plain = ask_capped(None, PLAIN);
    assert!(!plain.because.is_empty());
    let names = missing(&plain);
    // One name per withheld entry, the shape P3 answers with (the oracle).
    let withheld = evidence_ids(&plain).len();
    assert_eq!(withheld, 1);
    assert!(
        names
            .iter()
            .filter(|name| name.as_str() == "kmp_write_memory:traveler-1")
            .count()
            > 1,
        "{names:?}"
    );
}
