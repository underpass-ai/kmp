//! The anchored safety table (DISENO §L3) and the rest of the gate, through
//! `ask_response_from_result` exactly as a store that opted in reads them.
use std::collections::BTreeMap;

use kmp_application::{GetContextResult, MemoryAnswerPolicy, queries::render_graph_bundle};
use kmp_domain::{
    BundleMetadata, BundleNode, BundleRelationship, CaseId, KmpBundle, RelationExplanation,
    RelationSemanticClass, Role, TemporalSelection,
};
use kmp_proto::v1beta1::{AnswerStatus, AskResponse, MemoryConfidence, UnknownReason};

use super::anchor_selection::AnchorSelection;
use super::anchor_strength::AnchorStrength;
use super::morphology::Morphology;
use super::question_contract::QuestionContract;
use super::question_form::QuestionForm;
use super::question_time::QuestionTime;
use super::responses::UNANSWERED;
use super::{AskGate, AskRetrievalContext, LexicalBridge, ask_response_from_result};

/// A store of one about whose entries are `(ref, entry_kind, text)`.
fn store(entries: &[(&str, &str, &str)]) -> GetContextResult {
    store_related(entries, &[])
}

/// The same, with proven relations `(source, relation, target)` between the
/// entries.
pub(super) fn store_related(
    entries: &[(&str, &str, &str)],
    related: &[(&str, &str, &str)],
) -> GetContextResult {
    let node = |id: &str, kind: &str, summary: &str, entry_kind: Option<&str>| {
        let properties = entry_kind
            .map(|kind| BTreeMap::from([("entry_kind".to_string(), kind.to_string())]))
            .unwrap_or_default();
        BundleNode::new(id, kind, id, summary, "ACTIVE", Vec::new(), properties)
    };
    let mut nodes = vec![node("timeline:main", "memory_dimension", "Timeline", None)];
    let mut relationships = Vec::new();
    for (sequence, (id, kind, text)) in entries.iter().enumerate() {
        nodes.push(node(id, kind, text, Some(kind)));
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
    for (source, relation, target) in related {
        relationships.push(BundleRelationship::new(
            *source,
            *target,
            *relation,
            RelationExplanation::new(RelationSemanticClass::Evidential)
                .with_rationale("the writer checked both describe one change")
                .with_evidence("review notes 12")
                .with_confidence("high"),
        ));
    }
    let bundle = KmpBundle::new(
        CaseId::new("project:atlas").expect("case id"),
        Role::new("answerer").expect("role"),
        node("project:atlas", "memory_anchor", "Atlas memory", None),
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

fn ask_with(gate: Option<AskGate>, question: &str, entries: &[(&str, &str, &str)]) -> AskResponse {
    ask_in(gate, question, store(entries))
}

pub(super) fn ask_in(
    gate: Option<AskGate>,
    question: &str,
    store: GetContextResult,
) -> AskResponse {
    let context = AskRetrievalContext::from(store);
    let context = match gate {
        Some(gate) => context.with_gate(gate),
        None => context,
    };
    ask_response_from_result(
        question,
        None,
        MemoryAnswerPolicy::EvidenceOrUnknown,
        None,
        context,
        &LexicalBridge::none(),
        &TemporalSelection::Frontier,
    )
    .expect("an ask response")
}

pub(super) fn ask(question: &str, entries: &[(&str, &str, &str)]) -> AskResponse {
    ask_with(Some(AskGate::anchored(true)), question, entries)
}

pub(super) fn status(response: &AskResponse) -> AnswerStatus {
    AnswerStatus::try_from(response.answer_status).expect("a status")
}

pub(super) fn reason(response: &AskResponse) -> UnknownReason {
    UnknownReason::try_from(response.unknown_reason).expect("a reason")
}

pub(super) fn cited(response: &AskResponse) -> Vec<&str> {
    response
        .because
        .iter()
        .map(|reason| reason.claim.as_str())
        .collect()
}

pub(super) fn missing(response: &AskResponse) -> Vec<String> {
    response.proof.as_ref().expect("proof").missing.clone()
}

pub(super) fn confidence(response: &AskResponse) -> MemoryConfidence {
    MemoryConfidence::try_from(response.proof.as_ref().expect("proof").confidence)
        .expect("a confidence")
}

const ATLAS: &[(&str, &str, &str)] = &[
    (
        "decision:c64-adapter",
        "decision",
        "C6.4 local execution adapter runs ceremony steps inside the operator sandbox.",
    ),
    (
        "decision:c64-contract",
        "decision",
        "C6.4 contract: the local execution adapter returns a receipt within thirty seconds.",
    ),
    (
        "observation:c64-pending",
        "observation",
        "C6.4 pending work: the retry policy of the local execution adapter is not implemented yet.",
    ),
    (
        "decision:c65-scheduler",
        "decision",
        "C6.5 remote scheduler dispatches queued ceremonies to the worker pool.",
    ),
    (
        "observation:i188-preflight",
        "observation",
        "Issue #188 pause resume preflight checks the journal before a ceremony resumes.",
    ),
    (
        "observation:i188-deploy",
        "observation",
        "Issue #188 was deployed to the staging environment on Tuesday.",
    ),
    (
        "observation:billing-cluster",
        "observation",
        "The Kubernetes cluster for the billing service was upgraded to version 1.30.",
    ),
    (
        "decision:c613-scope",
        "decision",
        "C6.13 scope covers the pause and resume verbs of the ceremony runner.",
    ),
    (
        "decision:c7-limit",
        "decision",
        "C7 limit: the artifact store rejects uploads above two gigabytes.",
    ),
    (
        "decision:c7-scope",
        "decision",
        "C7 scope covers the migration of the artifact store.",
    ),
];

// --- The safety table -----------------------------------------------------

#[test]
fn an_attribute_absent_beside_its_anchor_is_unknown_and_named() {
    let question = "Which database engine was used when CI workflow #42 concluded and the remote branch remained present?";
    let context = [(
        "observation:ci-42",
        "observation",
        "CI workflow #42 concluded and the remote branch remained present.",
    )];

    let response = ask(question, &context);

    assert_eq!(response.answer, UNANSWERED);
    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
    assert_eq!(missing(&response), ["database", "engine"]);
    assert!(response.because.is_empty());
    assert_eq!(confidence(&response), MemoryConfidence::Unknown);

    // The twin that states it is answered from it alone.
    let answered = ask(
        question,
        &[
            context[0],
            (
                "decision:ci-42-engine",
                "decision",
                "CI workflow #42 ran its migrations against the PostgreSQL database engine.",
            ),
        ],
    );
    assert_eq!(status(&answered), AnswerStatus::Answered);
    assert_eq!(cited(&answered)[0], "decision:ci-42-engine");
}

#[test]
fn a_prefix_twin_of_the_anchor_is_unknown() {
    let response = ask(
        "Was kernel prefix #7 a deliberate decision?",
        &[(
            "observation:prefer-7",
            "observation",
            "The kernel prefer mode #7 delivered a decision from another workflow.",
        )],
    );

    assert_eq!(response.answer, UNANSWERED);
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
    assert!(missing(&response).contains(&"prefix".to_string()));
}

/// A0: the gate simulation turned this UNKNOWN into a whole ANSWER because it
/// did not ask for co-occurrence. `kubernetes` is in the about, in an entry
/// that does not name #188.
#[test]
fn a_concept_stated_only_in_another_entry_does_not_answer_the_anchor() {
    let response = ask(
        "Which Kubernetes cluster was issue #188 deployed to?",
        ATLAS,
    );

    assert_eq!(response.answer, UNANSWERED);
    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
    // In the reader's words, as written.
    assert_eq!(missing(&response), ["Kubernetes", "cluster"]);

    // The same question without the gate is what the table guards against
    // regressing into; the gate never needs it to have answered.
    let singular = ask("Where was issue #188 deployed?", ATLAS);
    assert_eq!(status(&singular), AnswerStatus::Answered);
    assert_eq!(cited(&singular)[0], "observation:i188-deploy");
    assert!(cited(&singular).iter().all(|claim| claim.contains("i188")));
}

#[test]
fn a_negated_anchor_is_not_required_and_cannot_be_cited_alone() {
    let question = "What exact C6.13 scope is recorded, excluding any C7 work?";
    let contract = QuestionContract::read(question, &Morphology::none());
    let c7 = contract
        .anchors()
        .iter()
        .find(|anchor| anchor.term == "c7")
        .expect("the excluded anchor is read");
    assert!(c7.negated && !c7.is_required());

    let response = ask(question, ATLAS);

    assert_eq!(status(&response), AnswerStatus::Answered);
    assert_eq!(cited(&response), ["decision:c613-scope"]);
    assert!(
        !cited(&response)
            .iter()
            .any(|claim| claim.starts_with("decision:c7")),
        "an entry about C7 alone entered the core"
    );
}

#[test]
fn quantities_are_soft_anchors_and_the_current_rule_decides() {
    let question = "Why did the request timeout move from 300 to 30 seconds?";
    let contract = QuestionContract::read(question, &Morphology::none());
    assert!(
        contract
            .anchors()
            .iter()
            .all(|anchor| anchor.strength == AnchorStrength::Soft),
        "{:?}",
        contract.anchors()
    );
    assert!(!contract.requires_anchors());

    let entries = [(
        "decision:timeout",
        "decision",
        "The request timeout moved from 300 to 30 seconds because workers hung.",
    )];
    let gated = ask(question, &entries);
    let plain = ask_with(None, question, &entries);
    assert_eq!(gated.answer, plain.answer);
    assert_eq!(gated.because, plain.because);
    assert_eq!(gated.proof, plain.proof);
    assert_eq!(status(&gated), AnswerStatus::Answered);
}

// --- The rest of the gate --------------------------------------------------

#[test]
fn an_anchor_no_candidate_names_is_absent_in_the_selection() {
    for question in [
        "C6.24 local execution adapter",
        "Which decisions, contracts, limits and pending work exist for C6.24?",
    ] {
        let response = ask(question, ATLAS);

        assert_eq!(response.answer, UNANSWERED, "{question}");
        assert_eq!(reason(&response), UnknownReason::AnchorAbsentInSelection);
        assert_eq!(missing(&response), ["C6.24"]);
        assert!(response.summary.ends_with("; not found: C6.24"));
    }
    // The guide word is part of how the reader named it.
    let response = ask("issue #288 pause resume preflight", ATLAS);
    assert_eq!(reason(&response), UnknownReason::AnchorAbsentInSelection);
    assert_eq!(missing(&response), ["issue #288"]);
}

#[test]
fn an_enumeration_answers_what_it_found_and_names_the_rest() {
    let question = "What rollback plan and retry policy are recorded for C6.4?";

    let partial = ask(question, ATLAS);
    assert_eq!(status(&partial), AnswerStatus::Partial);
    assert_ne!(partial.answer, UNANSWERED);
    assert_eq!(cited(&partial)[0], "observation:c64-pending");
    assert!(
        cited(&partial).iter().all(|claim| claim.contains("c64")),
        "{:?}",
        cited(&partial)
    );
    assert_eq!(&missing(&partial)[..2], ["rollback", "plan"]);
    assert_ne!(confidence(&partial), MemoryConfidence::High);
    assert!(partial.summary.starts_with("Retrieved"));
    assert!(partial.summary.contains("for part of:"));

    let strict = ask_with(Some(AskGate::anchored(false)), question, ATLAS);
    assert_eq!(status(&strict), AnswerStatus::Unknown);
    assert_eq!(reason(&strict), UnknownReason::AttributeNotFound);
}

#[test]
fn an_enumeration_whose_facets_the_anchor_states_is_answered() {
    let response = ask(
        "Which decisions, contracts, limits and pending work exist for C6.4?",
        ATLAS,
    );

    assert_eq!(status(&response), AnswerStatus::Answered);
    let mut claims = cited(&response);
    claims.sort_unstable();
    assert_eq!(
        claims,
        [
            "decision:c64-adapter",
            "decision:c64-contract",
            "observation:c64-pending"
        ]
    );
}

#[test]
fn a_neighbour_of_the_anchor_does_not_answer_for_it() {
    let response = ask("What retry policy is pending for C6.5?", ATLAS);

    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
    assert_eq!(missing(&response), ["retry", "policy"]);
}

#[test]
fn without_the_gate_nothing_is_reported_and_the_rule_is_unchanged() {
    let plain = ask_with(None, "issue #288 pause resume preflight", ATLAS);

    assert_eq!(plain.answer_status, AnswerStatus::Unspecified as i32);
    assert_eq!(plain.unknown_reason, UnknownReason::Unspecified as i32);
    // The ⌈2/3⌉ rule answers the twin, which is what the gate is for.
    assert_ne!(plain.answer, UNANSWERED);
    let value = crate::v1beta1::recall_projection::ask_value(&plain);
    assert!(value.get("answer_status").is_none());
    assert!(value.get("unknown_reason").is_none());

    let gated = crate::v1beta1::recall_projection::ask_value(&ask(
        "issue #288 pause resume preflight",
        ATLAS,
    ));
    assert_eq!(gated["answer_status"], "unknown");
    assert_eq!(gated["unknown_reason"], "anchor_absent_in_selection");
}

#[test]
fn a_question_without_anchors_still_says_how_it_settled() {
    let answered = ask("Which ceremony verbs does the runner pause?", ATLAS);
    assert_eq!(status(&answered), AnswerStatus::Answered);
    assert_eq!(reason(&answered), UnknownReason::Unspecified);

    let unrelated = ask("Which ocean hosts the coral expedition?", ATLAS);
    assert_eq!(status(&unrelated), AnswerStatus::Unknown);
    assert_eq!(reason(&unrelated), UnknownReason::NoCandidates);

    let empty = ask("Which ocean hosts the coral expedition?", &[]);
    assert_eq!(reason(&empty), UnknownReason::NoCandidates);
}

#[test]
fn a_negated_anchor_keeps_only_its_own_entries_out_of_the_core_without_a_required_one() {
    let entries = [
        (
            "decision:runner-limit",
            "decision",
            "The ceremony runner limit is seven paused ceremonies.",
        ),
        (
            "decision:c7-runner-limit",
            "decision",
            "C7 ceremony runner limit is two paused ceremonies.",
        ),
    ];
    let question = "What limit applies to the ceremony runner, excluding C7?";

    // Without the gate the ⌈2/3⌉ rule counts `excluding` and `C7` among the
    // concepts, so only the C7 entry clears it.
    let plain = ask_with(None, question, &entries);
    assert_eq!(cited(&plain), ["decision:c7-runner-limit"]);

    // The gate does not ask for what the question excluded: the rule reads
    // the question without it, and the entry whose only anchor is C7 stays
    // out of the core.
    let response = ask(question, &entries);

    assert_eq!(status(&response), AnswerStatus::Answered);
    assert_eq!(cited(&response), ["decision:runner-limit"]);
}

#[test]
fn an_entry_that_names_an_asked_anchor_besides_the_excluded_one_stays_in_the_core() {
    let entries = [
        (
            "decision:c613-with-c7",
            "decision",
            "C6.13 scope covers pause and resume, and C7 inherits its journal format.",
        ),
        (
            "decision:c7-scope",
            "decision",
            "C7 scope covers the migration of the artifact store.",
        ),
    ];
    let response = ask("What scope does C6.13 cover, excluding C7?", &entries);

    assert_eq!(status(&response), AnswerStatus::Answered);
    assert_eq!(cited(&response), ["decision:c613-with-c7"]);
}

#[test]
fn facets_break_ties_by_entry_kind_under_the_anchor() {
    let text = "C6.4 retry policy allows three attempts.";
    let entries = [
        ("a-observation:retry", "observation", text),
        ("b-decision:retry", "decision", text),
    ];

    let response = ask("Which decisions exist for the C6.4 retry policy?", &entries);

    assert_eq!(status(&response), AnswerStatus::Answered);
    assert_eq!(cited(&response)[0], "b-decision:retry");
    let plain = ask_with(
        None,
        "Which decisions exist for the C6.4 retry policy?",
        &entries,
    );
    assert_eq!(cited(&plain)[0], "a-observation:retry");
}

// --- The contract ----------------------------------------------------------

#[test]
fn the_contract_reads_anchors_by_strength() {
    let read = |question: &str| QuestionContract::read(question, &Morphology::none());
    let strengths = |question: &str| {
        read(question)
            .anchors()
            .iter()
            .map(|anchor| (anchor.term.clone(), anchor.strength))
            .collect::<Vec<_>>()
    };
    use AnchorStrength::{Hard, Soft};

    assert_eq!(
        strengths("What is stored for C6.4?"),
        [("c6.4".into(), Hard)]
    );
    assert_eq!(strengths("issue #188 status"), [("188".into(), Hard)]);
    assert_eq!(strengths("issue 18 status"), [("18".into(), Hard)]);
    assert_eq!(strengths("the 18 open items"), [("18".into(), Soft)]);
    assert_eq!(
        strengths("what failed at 300 calls"),
        [("300".into(), Soft)]
    );
    assert_eq!(strengths("request 300 status"), [("300".into(), Hard)]);
    assert_eq!(strengths("the 30s timeout"), [("30s".into(), Soft)]);
    assert_eq!(strengths("what shipped in 2026"), [("2026".into(), Soft)]);
    assert_eq!(
        strengths("what happened on 2026-09-26"),
        [("2026.09.26".into(), Soft)]
    );
    assert_eq!(
        strengths("C6.8+C6.9 review"),
        [("c6.8".into(), Hard), ("c6.9".into(), Hard)]
    );
    assert_eq!(
        strengths("C6.1-C6.4 scope"),
        [("c6.1".into(), Soft), ("c6.4".into(), Soft)]
    );
    assert!(strengths("which engine is current").is_empty());
}

#[test]
fn the_contract_reads_negation_to_the_end_of_its_clause() {
    let contract = QuestionContract::read(
        "Which limits apply to C6.4 without C6.5, and what does C6.6 add?",
        &Morphology::none(),
    );
    let negated = contract
        .anchors()
        .iter()
        .map(|anchor| (anchor.term.as_str(), anchor.negated))
        .collect::<Vec<_>>();

    assert_eq!(negated, [("c6.4", false), ("c6.5", true), ("c6.6", false)]);
    assert_eq!(
        contract.negated_terms().into_iter().collect::<Vec<_>>(),
        ["c6.5"]
    );
    let spanish = QuestionContract::read(
        "que limites hay para C6.4 fuera de C6.5",
        &Morphology::none(),
    );
    assert!(spanish.anchors()[1].negated);
}

#[test]
fn the_contract_reads_subject_facets_form_and_time() {
    let morphology = Morphology::none();
    let enumeration = QuestionContract::read(
        "Which decisions, contracts, limits and pending work exist for C6.4?",
        &morphology,
    );
    assert_eq!(enumeration.form(), QuestionForm::Enumerative);
    assert_eq!(
        enumeration.facets().iter().cloned().collect::<Vec<_>>(),
        [
            "facet:contracts",
            "facet:decisions",
            "facet:limits",
            "facet:pending"
        ]
    );
    assert_eq!(
        enumeration
            .subject()
            .iter()
            .map(|(_, word)| word.clone())
            .collect::<Vec<_>>(),
        ["work"]
    );
    assert!(enumeration.facet_entry_kinds().contains("decision"));

    let singular = QuestionContract::read(
        "Which database engine was used when CI workflow #42 concluded?",
        &morphology,
    );
    assert_eq!(singular.form(), QuestionForm::Singular);
    assert_eq!(
        singular
            .subject()
            .iter()
            .map(|(_, word)| word.clone())
            .collect::<Vec<_>>(),
        ["database", "engine"]
    );
    assert_eq!(singular.time(), QuestionTime::Unstated);

    let existence = QuestionContract::read("Is anything stored about #188?", &morphology);
    assert_eq!(existence.form(), QuestionForm::Enumerative);
    assert!(existence.subject().is_empty());

    let listed = QuestionContract::read(
        "What do we know about C6.4: the cache engine, the owner and the deadline?",
        &morphology,
    );
    assert_eq!(listed.form(), QuestionForm::Enumerative);

    assert_eq!(
        QuestionContract::read("What replaced the C6.4 adapter?", &morphology).time(),
        QuestionTime::History
    );
    assert_eq!(
        QuestionContract::read("What is the current C6.4 adapter?", &morphology).time(),
        QuestionTime::State
    );
    // A question that opens with its context word asks with all of it.
    let when = QuestionContract::read("When was issue #188 deployed?", &morphology);
    assert_eq!(
        when.subject()
            .iter()
            .map(|(_, word)| word.clone())
            .collect::<Vec<_>>(),
        ["deployed"]
    );
}

#[test]
fn the_principal_anchor_is_the_rarest_and_hubs_are_words() {
    let contract = QuestionContract::read("C6.4 and #188 review", &Morphology::none());
    let frequency =
        |c64: usize, i188: usize| move |term: &str| if term == "c6.4" { c64 } else { i188 };

    match AnchorSelection::read(&contract, frequency(3, 1), 40) {
        AnchorSelection::Anchored { principal, others } => {
            assert_eq!(principal.term, "188");
            assert_eq!(others.len(), 1);
            assert_eq!(others[0].term, "c6.4");
        }
        other => panic!("{other:?}"),
    }
    // A tie goes to the anchor named first.
    match AnchorSelection::read(&contract, frequency(2, 2), 40) {
        AnchorSelection::Anchored { principal, .. } => assert_eq!(principal.term, "c6.4"),
        other => panic!("{other:?}"),
    }
    // `c6.4` in 60 of 200 candidates is a hub, read as a word.
    match AnchorSelection::read(&contract, frequency(60, 2), 200) {
        AnchorSelection::Anchored { principal, others } => {
            assert_eq!(principal.term, "188");
            assert!(others.is_empty());
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        AnchorSelection::read(&contract, frequency(60, 70), 200),
        AnchorSelection::Unanchored
    );
    assert_eq!(
        AnchorSelection::read(&contract, frequency(0, 1), 200),
        AnchorSelection::Absent(vec!["C6.4".to_string()])
    );
}

#[test]
fn an_anchor_most_of_the_selection_names_is_a_hub_and_the_current_rule_decides() {
    let texts = (0..12)
        .map(|index| format!("C6.4 rollout step {index} ran the canary in zone {index}."))
        .collect::<Vec<_>>();
    let refs = (0..12)
        .map(|index| format!("observation:step-{index}"))
        .collect::<Vec<_>>();
    let entries = refs
        .iter()
        .zip(&texts)
        .map(|(id, text)| (id.as_str(), "observation", text.as_str()))
        .collect::<Vec<_>>();
    let question = "Which rollout step of C6.4 ran the canary?";

    let gated = ask(question, &entries);
    let plain = ask_with(None, question, &entries);

    assert_eq!(gated.answer, plain.answer);
    assert_eq!(gated.because, plain.because);
    assert_eq!(gated.proof, plain.proof);
    assert_eq!(status(&gated), AnswerStatus::Answered);
}
