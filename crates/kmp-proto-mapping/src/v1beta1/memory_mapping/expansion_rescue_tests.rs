//! Judged search expansions (P15, Doc2Query--) through
//! `ask_response_from_result`, with and without the anchored gate: a memory
//! the question reaches only through its expansions comes back outside the
//! core, marked, and cited by its own text.
use std::collections::BTreeMap;

use kmp_application::{GetContextResult, MemoryAnswerPolicy, queries::render_graph_bundle};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, KmpBundle, RelationExplanation,
    RelationSemanticClass, Role, SearchExpansions, SearchSummary, TemporalSelection,
};
use kmp_proto::v1beta1::{AnswerStatus, AskResponse, MemoryEvidence};

use super::anchored_gate_tests::{ask_in, cited, status};
use super::answer_selection::{EXPANSION_TERMS_KEY, REACHED_BY_EXPANSION, REACHED_BY_KEY};
use super::expansion_rescue::MAX_EXPANDED_CANDIDATES;
use super::{AskGate, AskRetrievalContext, LexicalBridge, ask_response_from_result};

/// `(ref, text, expansions)`: expansions bound to the text by its
/// fingerprint, as a write that a judge accepted stores them.
type Entry<'a> = (&'a str, &'a str, &'a [&'a str]);

fn store(entries: &[Entry<'_>]) -> GetContextResult {
    store_bound(entries, true)
}

fn store_bound(entries: &[Entry<'_>], bound: bool) -> GetContextResult {
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
    for (sequence, (id, text, expansions)) in entries.iter().enumerate() {
        let mut properties = BTreeMap::from([("entry_kind".to_string(), "decision".to_string())]);
        if !expansions.is_empty() {
            let fingerprint = if bound {
                SearchSummary::source_fingerprint(text)
            } else {
                SearchSummary::source_fingerprint("another text")
            };
            let metadata = BTreeMap::from([
                (SearchExpansions::METADATA_KEY, expansions.join("\n")),
                (
                    SearchExpansions::SOURCE_FINGERPRINT_METADATA_KEY,
                    fingerprint,
                ),
                (
                    SearchExpansions::JUDGED_BY_METADATA_KEY,
                    "jev-1.13.0 noul>=0.80".to_string(),
                ),
            ]);
            properties.insert(
                "metadata".to_string(),
                serde_json::to_string(&metadata).expect("metadata"),
            );
        }
        nodes.push(node(id, "decision", text, properties));
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
        CaseId::new("project:paraphrase").expect("case id"),
        Role::new("answerer").expect("role"),
        node(
            "project:paraphrase",
            "memory_anchor",
            "Paraphrase memory",
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

fn ask_policy(question: &str, policy: MemoryAnswerPolicy, store: GetContextResult) -> AskResponse {
    ask_response_from_result(
        question,
        None,
        policy,
        None,
        AskRetrievalContext::from(store),
        &LexicalBridge::none(),
        &TemporalSelection::Frontier,
    )
    .expect("an ask response")
}

fn evidence(response: &AskResponse) -> &[MemoryEvidence] {
    &response.proof.as_ref().expect("proof").evidence
}

fn find<'r>(response: &'r AskResponse, entry: &str) -> Option<&'r MemoryEvidence> {
    evidence(response)
        .iter()
        .find(|item| item.id == format!("entry:{entry}"))
}

const ROLLOUT: &str = "The rollout slipped because the auditors had not signed off.";
const CANTEEN: &str = "The canteen menu changed on Tuesday for the whole building.";

fn paraphrase_store(expansions: &[&str]) -> GetContextResult {
    store(&[
        ("mem:rollout", ROLLOUT, expansions),
        ("mem:canteen", CANTEEN, &[]),
    ])
}

const JUDGED: &[&str] = &[
    "Why was the launch postponed?",
    "launch delay audit",
    "¿Por qué se retrasó el lanzamiento?",
];

#[test]
fn a_question_reached_only_through_expansions_is_rescued_outside_the_core() {
    let response = ask_in(
        None,
        "Why was the launch postponed?",
        paraphrase_store(JUDGED),
    );

    let rescued = find(&response, "mem:rollout").expect("the rollout memory is rescued");
    assert_eq!(rescued.text, ROLLOUT, "the citation is the stored text");
    assert_eq!(rescued.metadata[REACHED_BY_KEY], REACHED_BY_EXPANSION);
    assert_eq!(rescued.metadata[EXPANSION_TERMS_KEY], "launch, postponed");
    assert!(
        rescued
            .metadata
            .keys()
            .all(|key| !SearchExpansions::is_metadata_key(key)),
        "the expansions are searched, never shown: {:?}",
        rescued.metadata
    );
    assert!(
        cited(&response).is_empty(),
        "outside the core: {response:?}"
    );
    assert!(find(&response, "mem:canteen").is_none());
}

#[test]
fn without_expansions_the_paraphrase_stays_unreached() {
    let response = ask_in(None, "Why was the launch postponed?", paraphrase_store(&[]));
    assert!(find(&response, "mem:rollout").is_none(), "{response:?}");
}

#[test]
fn a_key_in_the_other_language_reaches_the_memory() {
    let response = ask_in(
        None,
        "¿Por qué se retrasó el lanzamiento?",
        paraphrase_store(JUDGED),
    );
    let rescued = find(&response, "mem:rollout").expect("rescued across the language");
    assert_eq!(rescued.metadata[REACHED_BY_KEY], REACHED_BY_EXPANSION);
}

#[test]
fn expansions_bound_to_another_text_are_not_searched() {
    let response = ask_in(
        None,
        "Why was the launch postponed?",
        store_bound(
            &[
                ("mem:rollout", ROLLOUT, JUDGED),
                ("mem:canteen", CANTEEN, &[]),
            ],
            false,
        ),
    );
    assert!(find(&response, "mem:rollout").is_none(), "{response:?}");
}

#[test]
fn a_memory_its_own_words_reach_is_cited_as_usual() {
    let response = ask_policy(
        "Why did the rollout slip?",
        MemoryAnswerPolicy::BestEffort,
        paraphrase_store(JUDGED),
    );
    let direct = find(&response, "mem:rollout").expect("reached in its own words");
    assert!(
        !direct.metadata.contains_key(REACHED_BY_KEY),
        "{:?}",
        direct.metadata
    );
    assert!(!direct.metadata.contains_key(EXPANSION_TERMS_KEY));
    assert_eq!(cited(&response), ["mem:rollout"]);
}

#[test]
fn an_expansion_never_satisfies_an_anchor_under_the_gate() {
    let response = ask_in(
        Some(AskGate::anchored(true)),
        "Why was release v2.4 postponed?",
        store(&[
            ("mem:rollout", ROLLOUT, &["Why was release v2.4 postponed?"]),
            ("mem:canteen", CANTEEN, &[]),
        ]),
    );
    assert_ne!(status(&response), AnswerStatus::Answered, "{response:?}");
    assert!(cited(&response).is_empty(), "{response:?}");
}

#[test]
fn expansions_bring_in_at_most_their_cap() {
    let entries = (0..6)
        .map(|index| {
            (
                format!("mem:rollout-{index}"),
                format!("Rollout number {index} slipped after the audit."),
            )
        })
        .collect::<Vec<_>>();
    let borrowed = entries
        .iter()
        .map(|(id, text)| (id.as_str(), text.as_str(), JUDGED))
        .collect::<Vec<_>>();
    let response = ask_in(None, "Why was the launch postponed?", store(&borrowed));
    let rescued = evidence(&response)
        .iter()
        .filter(|item| {
            item.metadata.get(REACHED_BY_KEY).map(String::as_str) == Some(REACHED_BY_EXPANSION)
        })
        .count();
    assert_eq!(rescued, MAX_EXPANDED_CANDIDATES);
}

#[test]
fn a_strict_focus_the_expansions_do_not_answer_rescues_nothing() {
    let response = ask_in(
        None,
        "Why was the canteen launch postponed by the kitchen staff?",
        paraphrase_store(&["launch postponed"]),
    );
    assert!(
        evidence(&response).iter().all(|item| {
            item.metadata.get(REACHED_BY_KEY).map(String::as_str) != Some(REACHED_BY_EXPANSION)
        }),
        "{response:?}"
    );
}
