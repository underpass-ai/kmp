//! The lifecycle rescue (DISENO §L5, P7) through `ask_response_from_result`,
//! with and without the anchored gate.
use kmp_proto::v1beta1::{AnswerStatus, AskResponse, MemoryEvidence};

use super::AskGate;
use super::anchored_gate_tests::{ask_in, cited, status, store_related};
use super::answer_selection::{LIFECYCLE_HEADS_KEY, LIFECYCLE_STATE_KEY, REACHED_BY_KEY};

fn evidence(response: &AskResponse) -> &[MemoryEvidence] {
    &response.proof.as_ref().expect("proof").evidence
}

fn find<'r>(response: &'r AskResponse, entry: &str) -> Option<&'r MemoryEvidence> {
    evidence(response)
        .iter()
        .find(|item| item.id == format!("entry:{entry}"))
}

fn meta<'r>(item: &'r MemoryEvidence, key: &str) -> Option<&'r str> {
    item.metadata.get(key).map(String::as_str)
}

fn ungated(question: &str, store: kmp_application::GetContextResult) -> AskResponse {
    ask_in(None, question, store)
}

const QUEUE: &[(&str, &str, &str)] = &[
    (
        "mem:queue-v1",
        "decision",
        "The Atlas ingest queue runs on RabbitMQ.",
    ),
    (
        "mem:queue-v2",
        "decision",
        "Switched to NATS after the load test.",
    ),
    (
        "mem:lunch",
        "observation",
        "The team lunch moved to Thursday.",
    ),
];

#[test]
fn a_question_that_matches_a_replaced_memory_brings_its_current_head() {
    let store = store_related(QUEUE, &[("mem:queue-v2", "supersedes", "mem:queue-v1")]);

    let response = ungated("Which queue does the Atlas ingest run on?", store);

    let head = find(&response, "mem:queue-v2").expect("the successor is in the proof");
    assert_eq!(meta(head, REACHED_BY_KEY), Some("lifecycle"));
    assert_eq!(meta(head, "reached_from"), Some("mem:queue-v1"));
    assert_eq!(meta(head, "reached_via"), Some("supersedes"));
    // A question about now brings only standing heads, one hop away here:
    // neither goes said, since every page may repeat the largest item.
    assert_eq!(meta(head, "reached_hops"), None);
    assert_eq!(meta(head, LIFECYCLE_STATE_KEY), None);
    assert!(
        find(&response, "mem:queue-v1").is_none(),
        "a replaced memory never comes back as current"
    );
    assert!(
        !cited(&response).contains(&"mem:queue-v2"),
        "succession is not an answer: the head stays outside the core"
    );
    let proof = response.proof.as_ref().expect("proof");
    assert!(
        proof.superseded.is_empty() && proof.path.is_empty(),
        "the route travels on the item; it adds nothing every page repeats"
    );
}

#[test]
fn without_a_declared_lifecycle_nothing_is_rescued() {
    let store = store_related(QUEUE, &[]);

    let response = ungated("Which queue does the Atlas ingest run on?", store);

    assert!(
        evidence(&response)
            .iter()
            .all(|item| meta(item, REACHED_BY_KEY) != Some("lifecycle"))
    );
    assert!(find(&response, "mem:queue-v2").is_none());
}

#[test]
fn a_chain_of_corrections_brings_only_its_head() {
    let entries = &[
        (
            "mem:v1",
            "decision",
            "The Atlas ingest queue runs on RabbitMQ.",
        ),
        (
            "mem:v2",
            "decision",
            "Correction: the broker is RabbitMQ 3.12.",
        ),
        (
            "mem:v3",
            "decision",
            "Now on NATS JetStream after the load test.",
        ),
    ];
    let store = store_related(
        entries,
        &[
            ("mem:v2", "supersedes", "mem:v1"),
            ("mem:v3", "updates_state", "mem:v2"),
        ],
    );

    let response = ungated("Which queue does the Atlas ingest run on?", store);

    let head = find(&response, "mem:v3").expect("the head");
    assert_eq!(meta(head, "reached_via"), Some("updates_state"));
    assert_eq!(meta(head, "reached_hops"), Some("2"));
    assert!(
        find(&response, "mem:v2")
            .is_none_or(|item| meta(item, REACHED_BY_KEY) != Some("lifecycle")),
        "a member behind the head is not brought in as current"
    );
}

#[test]
fn a_successor_the_question_already_matched_is_not_brought_twice() {
    let entries = &[
        (
            "mem:v1",
            "decision",
            "The Atlas ingest queue runs on RabbitMQ.",
        ),
        ("mem:v2", "decision", "The Atlas ingest queue runs on NATS."),
    ];
    let store = store_related(entries, &[("mem:v2", "supersedes", "mem:v1")]);

    let response = ungated("Which queue does the Atlas ingest run on?", store);

    assert_eq!(cited(&response), ["mem:v2"]);
    let v2 = evidence(&response)
        .iter()
        .filter(|item| item.id == "entry:mem:v2")
        .collect::<Vec<_>>();
    assert_eq!(v2.len(), 1);
    assert_eq!(meta(v2[0], REACHED_BY_KEY), None);
}

#[test]
fn a_fork_brings_each_head_and_names_them() {
    let entries = &[
        (
            "mem:v1",
            "decision",
            "The Atlas ingest queue runs on RabbitMQ.",
        ),
        ("mem:east", "decision", "East region moved to NATS."),
        ("mem:west", "decision", "West region moved to Kafka."),
    ];
    let store = store_related(
        entries,
        &[
            ("mem:east", "supersedes", "mem:v1"),
            ("mem:west", "supersedes", "mem:v1"),
        ],
    );

    let response = ungated("Which queue does the Atlas ingest run on?", store);

    for head in ["mem:east", "mem:west"] {
        let item = find(&response, head).expect("each head");
        assert_eq!(meta(item, LIFECYCLE_HEADS_KEY), Some("mem:east,mem:west"));
    }
}

#[test]
fn a_question_about_now_brings_at_most_three_heads() {
    let mut entries = vec![(
        "mem:v1".to_string(),
        "The Atlas ingest queue runs on RabbitMQ.".to_string(),
    )];
    let mut related = Vec::new();
    for index in 0..5 {
        entries.push((
            format!("mem:fork-{index}"),
            format!("Region {index} moved away."),
        ));
        related.push((format!("mem:fork-{index}"), "mem:v1".to_string()));
    }
    let entries = entries
        .iter()
        .map(|(id, text)| (id.as_str(), "decision", text.as_str()))
        .collect::<Vec<_>>();
    let related = related
        .iter()
        .map(|(newer, older)| (newer.as_str(), "supersedes", older.as_str()))
        .collect::<Vec<_>>();

    let response = ungated(
        "Which queue does the Atlas ingest run on?",
        store_related(&entries, &related),
    );

    let rescued = evidence(&response)
        .iter()
        .filter(|item| meta(item, REACHED_BY_KEY) == Some("lifecycle"))
        .count();
    assert_eq!(rescued, 3);
}

#[test]
fn a_question_about_history_brings_the_chain_with_each_state() {
    let entries = &[
        (
            "mem:v1",
            "decision",
            "The Atlas ingest queue runs on RabbitMQ.",
        ),
        (
            "mem:v2",
            "decision",
            "Switched to NATS after the load test.",
        ),
        ("mem:v3", "decision", "Moved again, to NATS JetStream."),
    ];
    let store = store_related(
        entries,
        &[
            ("mem:v2", "supersedes", "mem:v1"),
            ("mem:v3", "supersedes", "mem:v2"),
        ],
    );

    let response = ungated("What is the history of the Atlas ingest queue?", store);

    let states = ["mem:v1", "mem:v2", "mem:v3"]
        .map(|entry| find(&response, entry).and_then(|item| meta(item, LIFECYCLE_STATE_KEY)));
    assert_eq!(
        states,
        [Some("replaced"), Some("replaced"), Some("current")]
    );
    assert!(
        cited(&response).is_empty() || !cited(&response).contains(&"mem:v1"),
        "a replaced member is proof of history, not current advice"
    );
}

#[test]
fn under_the_gate_the_head_of_a_replaced_memory_that_named_the_anchor_stays_in_the_proof() {
    let entries = &[
        (
            "mem:v1",
            "decision",
            "Service Q-7 ingests through RabbitMQ.",
        ),
        (
            "mem:v2",
            "decision",
            "Switched to NATS after the load test.",
        ),
    ];
    let store = store_related(entries, &[("mem:v2", "supersedes", "mem:v1")]);

    let response = ask_in(
        Some(AskGate::anchored(true)),
        "Which broker does service Q-7 ingest through?",
        store,
    );

    assert_eq!(status(&response), AnswerStatus::Unknown);
    let head = find(&response, "mem:v2").expect("the replacement is not filtered out");
    assert_eq!(meta(head, REACHED_BY_KEY), Some("lifecycle"));
    assert!(cited(&response).is_empty());
}

#[test]
fn the_variant_cites_the_head_for_the_anchor_its_predecessor_named() {
    // The successor states the subject and not the identifier: the gate
    // still demands every concept beside the anchor, and gets them.
    let entries = &[
        (
            "mem:v1",
            "decision",
            "Service Q-7 ingests through the RabbitMQ broker.",
        ),
        (
            "mem:v2",
            "decision",
            "The service now ingests through the NATS broker.",
        ),
    ];
    let related = &[("mem:v2", "supersedes", "mem:v1")];
    let question = "Which broker does service Q-7 ingest through?";

    let default = ask_in(
        Some(AskGate::anchored(true)),
        question,
        store_related(entries, related),
    );
    let variant = ask_in(
        Some(AskGate::anchored(true).with_successor_core(true)),
        question,
        store_related(entries, related),
    );

    assert!(cited(&default).is_empty());
    assert_eq!(cited(&variant), ["mem:v2"]);
    let head = find(&variant, "mem:v2").expect("cited");
    assert_eq!(meta(head, "anchor_via"), Some("supersedes"));
    assert_eq!(meta(head, "anchor_from"), Some("mem:v1"));
    assert_eq!(status(&variant), AnswerStatus::Answered);
}

#[test]
fn the_variant_still_demands_the_subject_beside_the_anchor() {
    let entries = &[
        (
            "mem:v1",
            "decision",
            "Service Q-7 ingests through the RabbitMQ broker.",
        ),
        (
            "mem:v2",
            "decision",
            "Switched to NATS after the load test.",
        ),
    ];
    let response = ask_in(
        Some(AskGate::anchored(true).with_successor_core(true)),
        "Which broker does service Q-7 ingest through?",
        store_related(entries, &[("mem:v2", "supersedes", "mem:v1")]),
    );

    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert!(cited(&response).is_empty());
}

#[test]
fn the_variant_never_cites_a_head_for_a_history_question() {
    let entries = &[
        (
            "mem:v1",
            "decision",
            "Service Q-7 ingests through the RabbitMQ broker.",
        ),
        (
            "mem:v2",
            "decision",
            "The broker is now NATS after the load test.",
        ),
    ];
    let response = ask_in(
        Some(AskGate::anchored(true).with_successor_core(true)),
        "Which broker did service Q-7 use previously?",
        store_related(entries, &[("mem:v2", "supersedes", "mem:v1")]),
    );

    assert!(
        !cited(&response).contains(&"mem:v2") || {
            let head = find(&response, "mem:v2").expect("cited");
            meta(head, "anchor_via").is_none()
        }
    );
}
