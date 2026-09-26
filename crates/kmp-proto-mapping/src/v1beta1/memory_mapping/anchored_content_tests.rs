//! What the anchored gate reads of a memory: its content and nothing else
//! (DISENO §13, P4 v2, 26 Sept 2026), and the stem that must not reach the
//! concept table through a derivation.
use std::collections::BTreeMap;

use kmp_application::{GetContextResult, queries::render_graph_bundle};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, KmpBundle, RelationExplanation,
    RelationSemanticClass, Role,
};
use kmp_proto::v1beta1::{AnswerStatus, UnknownReason};

use super::AskGate;
use super::anchored_gate_tests::{ask, ask_in, cited, missing, reason, status};
use super::morphology::Morphology;
use super::question_contract::QuestionContract;

/// English enough for the store to read as English and stem.
const BACKGROUND: &[(&str, &str, &str)] = &[
    (
        "observation:billing-cluster",
        "observation",
        "The Kubernetes cluster for the billing service was upgraded to version 1.30.",
    ),
    (
        "observation:i188-deploy",
        "observation",
        "Issue #188 was deployed to the staging environment on Tuesday.",
    ),
    (
        "decision:weekly-review",
        "decision",
        "The weekly review moved to Thursday because the team is travelling this month.",
    ),
];

/// A store of one about whose entries are `(ref, entry_kind, text,
/// source, metadata)`.
pub(super) fn store_with_properties(
    entries: &[(&str, &str, &str, &str, &str)],
) -> GetContextResult {
    let node = |id: &str, kind: &str, summary: &str, properties: BTreeMap<String, String>| {
        BundleNode::new(id, kind, id, summary, "ACTIVE", Vec::new(), properties)
    };
    let mut nodes = vec![node(
        "timeline:main",
        "memory_dimension",
        "Timeline",
        BTreeMap::new(),
    )];
    let mut relationships = Vec::new();
    for (sequence, (id, kind, text, source, metadata)) in entries.iter().enumerate() {
        let mut properties = BTreeMap::from([("entry_kind".to_string(), kind.to_string())]);
        if !source.is_empty() {
            properties.insert("source".to_string(), source.to_string());
        }
        if !metadata.is_empty() {
            properties.insert("payload_metadata".to_string(), metadata.to_string());
        }
        nodes.push(node(id, kind, text, properties));
        relationships.push(BundleRelationship::new(
            "timeline:main",
            *id,
            "contains_entry",
            RelationExplanation::new(RelationSemanticClass::Structural)
                .with_dimension("timeline")
                .with_scope_id("timeline:main")
                .with_sequence(sequence as u32 + 1),
        ));
    }
    let bundle = KmpBundle::new(
        CaseId::new("project:atlas").expect("case id"),
        Role::new("answerer").expect("role"),
        node(
            "project:atlas",
            "memory_anchor",
            "Atlas memory",
            BTreeMap::new(),
        ),
        nodes,
        relationships,
        Vec::new(),
        BundleMetadata::initial("test"),
    )
    .expect("bundle");
    let rendered = render_graph_bundle(&bundle);
    GetContextResult {
        read_revision: None,
        bundle,
        rendered,
        requested_scopes: Vec::new(),
        served_at: std::time::SystemTime::UNIX_EPOCH,
        timing: None,
    }
}

fn with_background<'a>(
    entries: &[(&'a str, &'a str, &'a str, &'a str, &'a str)],
) -> Vec<(&'a str, &'a str, &'a str, &'a str, &'a str)> {
    BACKGROUND
        .iter()
        .map(|(id, kind, text)| (*id, *kind, *text, "", ""))
        .chain(entries.iter().copied())
        .collect()
}

#[test]
fn an_anchor_only_the_ref_spells_does_not_put_a_memory_in_the_core() {
    // The ref's slug and the source say `issue 188`; the sentence does not.
    let store = store_with_properties(&with_background(&[(
        "observation:issue-188-cluster",
        "observation",
        "The Kubernetes cluster for the reporting service was pinned to three nodes.",
        "issue 188 review",
        "",
    )]));

    let response = ask_in(
        Some(AskGate::anchored(true)),
        "Which Kubernetes cluster was issue #188 deployed to?",
        store,
    );

    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
    assert_eq!(missing(&response), ["Kubernetes", "cluster"]);
    assert!(
        !cited(&response)
            .iter()
            .any(|claim| claim.contains("issue-188-cluster"))
    );
}

#[test]
fn an_anchor_no_memory_says_is_absent_however_its_refs_spell_it() {
    let store = store_with_properties(&with_background(&[(
        "observation:c42-note",
        "observation",
        "The reporting service was pinned to three nodes.",
        "cut C4.2 review",
        "",
    )]));

    let response = ask_in(
        Some(AskGate::anchored(true)),
        "How many nodes did C4.2 pin?",
        store,
    );

    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(reason(&response), UnknownReason::AnchorAbsentInSelection);
    assert_eq!(missing(&response), ["C4.2"]);
}

#[test]
fn a_concept_only_the_source_or_metadata_carries_is_not_stated() {
    // The memory names the anchor; what it was asked of it lives only in its
    // source and in a metadata value, which no reader is shown as its words.
    let store = store_with_properties(&with_background(&[(
        "observation:i188-rollout",
        "observation",
        "Issue #188 finished its rollout on Tuesday.",
        "rollback runbook",
        r#"{"stage":"rollback"}"#,
    )]));

    let response = ask_in(Some(AskGate::anchored(true)), "Issue #188 rollback?", store);

    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
    assert_eq!(missing(&response), ["rollback"]);
}

const CORRECTIONS: &[(&str, &str, &str)] = &[
    (
        "observation:pr-817-history",
        "observation",
        "PR #817 and its passing recovery and migration checks demonstrate the proposed correction of the card history.",
    ),
    (
        "observation:pr-819-release",
        "observation",
        "PR #819 fixed the release notes before the tag was published.",
    ),
    (
        "decision:weekly-review",
        "decision",
        "The weekly review moved to Thursday because the team is travelling this month.",
    ),
];

#[test]
fn a_derived_word_is_not_read_through_the_table_its_stem_lands_on() {
    let morphology = Morphology::read(CORRECTIONS.iter().map(|(_, _, text)| *text));
    let contract = QuestionContract::read("#817 correctness", &morphology);
    let concept = &contract.subject()[0];
    assert_eq!(concept.written, "correctness");
    assert!(concept.literal, "{concept:?}");

    // `correctness` stems to `correct`, which the table reads as `correction`.
    let response = ask("#817 correctness", CORRECTIONS);
    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
    assert_eq!(missing(&response), ["correctness"]);

    // An inflection keeps the table's reading: `fixes` is `fixed`.
    let contract = QuestionContract::read("Which fixes did PR #819 ship?", &morphology);
    assert!(contract.subject().iter().all(|concept| !concept.literal));
    let response = ask("PR #819 fixes?", CORRECTIONS);
    assert_eq!(status(&response), AnswerStatus::Answered);
    assert_eq!(cited(&response), ["observation:pr-819-release"]);
}
