//! The ask doubt band (DESIGN L4 4f, option B) through
//! `AskRetrievalContext::doubt_band` and `ask_response_from_result`, as the
//! MCP ask reads them: the band first, a judge's verdicts, then the answer.
use kmp_application::{GetContextResult, MemoryAnswerPolicy};
use kmp_domain::TemporalSelection;
use kmp_proto::v1beta1::{AnswerStatus, AskResponse, MemoryConfidence, UnknownReason};

use super::anchored_gate_tests::{cited, confidence, reason, status, store_related};
use super::doubt_band::DoubtBand;
use super::doubt_entry::DoubtEntry;
use super::doubt_judgement::DoubtJudgement;
use super::doubt_passage::DoubtPassage;
use super::doubt_verdicts::DoubtVerdicts;
use super::responses::UNANSWERED;
use super::{AskGate, AskRetrievalContext, LexicalBridge, ask_response_from_result};

const NEVER: i64 = i64::MIN;
const ALWAYS: i64 = i64::MAX;

const DEPLOYS: &[(&str, &str, &str)] = &[
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
];

fn store(entries: &[(&str, &str, &str)]) -> GetContextResult {
    store_related(entries, &[])
}

fn context(entries: &[(&str, &str, &str)]) -> AskRetrievalContext {
    AskRetrievalContext::from(store(entries)).with_gate(AskGate::anchored(true))
}

fn band_of(
    context: &mut AskRetrievalContext,
    question: &str,
    margin_below: i64,
) -> Option<DoubtBand> {
    context
        .doubt_band(
            question,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            &TemporalSelection::Frontier,
            &LexicalBridge::none(),
            margin_below,
        )
        .expect("a band reading")
}

fn answer(context: AskRetrievalContext, question: &str) -> AskResponse {
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

/// Verdicts on every passage of `band`: `answers(passage)` in thousandths,
/// the rest not answering. Veto at 800; promotion at 900 when `promote`.
fn judged(
    band: &DoubtBand,
    promote: bool,
    answers: impl Fn(&DoubtPassage) -> u16,
) -> DoubtVerdicts {
    let mut verdicts = DoubtVerdicts::new(
        band.entry,
        "jev-test".into(),
        "doubt_band.v1".into(),
        800,
        promote.then_some(900),
    )
    .expect("thresholds");
    for passage in &band.passages {
        let answers = answers(passage);
        verdicts
            .judge(
                &passage.id,
                &passage.text_sha256,
                DoubtJudgement {
                    answers,
                    not_answers: 1_000 - answers,
                },
            )
            .expect("judged");
    }
    verdicts
}

fn names(passage: &DoubtPassage, word: &str) -> bool {
    passage.text.contains(word)
}

fn marked<'r>(response: &'r AskResponse, key: &str) -> Vec<&'r str> {
    response
        .proof
        .as_ref()
        .expect("proof")
        .evidence
        .iter()
        .filter(|item| item.metadata.contains_key(key))
        .map(|item| {
            item.supports
                .first()
                .map_or(item.id.as_str(), String::as_str)
        })
        .collect()
}

// --- Entry ------------------------------------------------------------------

#[test]
fn an_attribute_not_found_enters_with_the_memories_that_name_the_anchor() {
    let question = "Which Kubernetes cluster was issue #188 deployed to?";
    let mut context = context(DEPLOYS);
    let band = band_of(&mut context, question, NEVER).expect("in the band");
    assert_eq!(band.entry, DoubtEntry::AttributeNotFound);
    assert!(band.passages.iter().all(|passage| names(passage, "#188")));
    assert!(band.passages.iter().all(|passage| !passage.in_core));
    assert!(band.passages.iter().all(|passage| passage.promotable));
    assert!(band.passages.len() <= 8);
    assert_eq!(
        band.passages[0].text_sha256.len(),
        64,
        "judged by the digest of the exact text"
    );
}

#[test]
fn a_clear_answer_stays_out_and_a_narrow_margin_enters() {
    let question = "Where was issue #188 deployed?";
    let mut clear = context(DEPLOYS);
    assert_eq!(band_of(&mut clear, question, NEVER), None);
    // Without verdicts the answer stands on the reading the band took.
    let reread = answer(clear, question);
    let plain = answer(context(DEPLOYS), question);
    assert_eq!(reread, plain);
    assert_eq!(status(&plain), AnswerStatus::Answered);

    let mut narrow = context(DEPLOYS);
    let band = band_of(&mut narrow, question, ALWAYS).expect("in the band");
    assert_eq!(band.entry, DoubtEntry::NarrowMargin);
    assert!(band.passages[0].in_core);
    assert_eq!(band.passages[0].entry_ref, "observation:i188-deploy");
}

#[test]
fn an_absent_anchor_best_effort_or_no_gate_never_enter() {
    let mut absent = context(DEPLOYS);
    assert_eq!(
        band_of(&mut absent, "Where was issue #288 deployed?", ALWAYS),
        None
    );
    let mut ungated = AskRetrievalContext::from(store(DEPLOYS));
    assert_eq!(
        band_of(&mut ungated, "Where was issue #188 deployed?", ALWAYS),
        None
    );
    let mut best_effort = context(DEPLOYS);
    let band = best_effort
        .doubt_band(
            "Where was issue #188 deployed?",
            MemoryAnswerPolicy::BestEffort,
            &TemporalSelection::Frontier,
            &LexicalBridge::none(),
            ALWAYS,
        )
        .expect("a reading");
    assert_eq!(band, None);
}

// --- B1, veto ---------------------------------------------------------------

#[test]
fn a_vetoed_citation_leaves_the_core_and_stays_in_the_proof_marked() {
    let question = "Where was issue #188 deployed?";
    let mut context = context(DEPLOYS);
    let band = band_of(&mut context, question, ALWAYS).expect("in the band");
    let verdicts = judged(&band, false, |passage| {
        if names(passage, "staging") { 50 } else { 500 }
    });
    let response = answer(context.with_doubt_verdicts(verdicts), question);
    assert!(!cited(&response).contains(&"observation:i188-deploy"));
    assert_eq!(marked(&response, "judged_out"), ["observation:i188-deploy"]);
    let vetoed = response
        .proof
        .as_ref()
        .expect("proof")
        .evidence
        .iter()
        .find(|item| item.metadata.contains_key("judged_out"))
        .expect("kept in the proof");
    assert_eq!(vetoed.metadata["judged_out"], "jev-test");
    assert_eq!(vetoed.metadata["judged_permille"], "950");
    assert_eq!(vetoed.metadata["judged_template"], "doubt_band.v1");
    assert!(
        response
            .warnings
            .iter()
            .any(|warning| warning.starts_with("doubt band (narrow_margin)")),
        "{:?}",
        response.warnings
    );
}

#[test]
fn a_veto_of_the_whole_core_is_unknown_and_never_answers_an_unknown() {
    let question = "Where was issue #188 deployed?";
    let mut context = context(DEPLOYS);
    let band = band_of(&mut context, question, ALWAYS).expect("in the band");
    let response = answer(
        context.with_doubt_verdicts(judged(&band, false, |_| 0)),
        question,
    );
    assert_eq!(response.answer, UNANSWERED);
    assert_eq!(status(&response), AnswerStatus::Unknown);
    assert_eq!(reason(&response), UnknownReason::NoBearing);

    // A veto on an UNKNOWN whose anchor was found leaves it UNKNOWN.
    let question = "Which Kubernetes cluster was issue #188 deployed to?";
    let mut context = self::context(DEPLOYS);
    let band = band_of(&mut context, question, NEVER).expect("in the band");
    let response = answer(
        context.with_doubt_verdicts(judged(&band, false, |_| 0)),
        question,
    );
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
}

// --- B2, promotion ------------------------------------------------------------

#[test]
fn a_promotion_answers_an_attribute_not_found_only_behind_its_flag() {
    let question = "Which Kubernetes cluster was issue #188 deployed to?";
    let judge = |passage: &DoubtPassage| if names(passage, "staging") { 950 } else { 400 };

    let mut off = context(DEPLOYS);
    let band = band_of(&mut off, question, NEVER).expect("in the band");
    let response = answer(
        off.with_doubt_verdicts(judged(&band, false, judge)),
        question,
    );
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
    assert!(marked(&response, "judged_by").is_empty());

    let mut on = context(DEPLOYS);
    let band = band_of(&mut on, question, NEVER).expect("in the band");
    let response = answer(on.with_doubt_verdicts(judged(&band, true, judge)), question);
    assert_eq!(status(&response), AnswerStatus::Answered);
    assert_eq!(cited(&response), ["observation:i188-deploy"]);
    assert_eq!(marked(&response, "judged_by"), ["observation:i188-deploy"]);
    assert_eq!(
        confidence(&response),
        MemoryConfidence::Medium,
        "a judged citation is never high"
    );
    assert!(response.proof.as_ref().expect("proof").missing.is_empty());
}

#[test]
fn a_promotion_never_reaches_a_memory_that_does_not_name_the_anchor() {
    let question = "Which Kubernetes cluster was issue #188 deployed to?";
    let mut context = context(DEPLOYS);
    let band = band_of(&mut context, question, NEVER).expect("in the band");
    // The billing cluster memory answers "which Kubernetes cluster" but is
    // not about #188: it is never in the band, so never promoted.
    assert!(
        band.passages
            .iter()
            .all(|passage| passage.entry_ref != "observation:billing-cluster")
    );
    let mut verdicts = judged(&band, true, |_| 400);
    let billing = "The Kubernetes cluster for the billing service was upgraded to version 1.30.";
    let sha = {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(billing.as_bytes()))
    };
    verdicts
        .judge(
            "observation:billing-cluster",
            &sha,
            DoubtJudgement {
                answers: 1_000,
                not_answers: 0,
            },
        )
        .expect("judged");
    let response = answer(context.with_doubt_verdicts(verdicts), question);
    assert_eq!(reason(&response), UnknownReason::AttributeNotFound);
}

// --- Without a deciding anchor -----------------------------------------------

const RUNBOOK: &[(&str, &str, &str)] = &[
    (
        "decision:cache-port",
        "decision",
        "The cache server listens on port 6380 behind the internal proxy.",
    ),
    (
        "observation:cache-rack",
        "observation",
        "The cache server moved to the second rack last week.",
    ),
    (
        "observation:proxy-tls",
        "observation",
        "The internal proxy terminates TLS for every service.",
    ),
];

#[test]
fn an_unanchored_unknown_with_a_best_effort_core_enters_and_a_promotion_answers() {
    let question = "Which firewall zone guards the cache endpoint appliance?";
    let mut context = context(RUNBOOK);
    let plain = answer(self::context(RUNBOOK), question);
    assert_eq!(status(&plain), AnswerStatus::Unknown, "{:?}", plain.summary);
    let band = band_of(&mut context, question, NEVER).expect("in the band");
    assert_eq!(band.entry, DoubtEntry::UnanchoredUnknown);
    assert!(band.passages.iter().all(|passage| !passage.in_core));

    // A veto never makes an answer.
    let vetoed = answer(
        self::context(RUNBOOK).with_doubt_verdicts(judged(&band, false, |_| 0)),
        question,
    );
    assert_eq!(status(&vetoed), AnswerStatus::Unknown);

    let promoted = answer(
        context.with_doubt_verdicts(judged(&band, true, |passage| {
            if names(passage, "6380") { 990 } else { 100 }
        })),
        question,
    );
    assert_eq!(status(&promoted), AnswerStatus::Answered);
    assert_eq!(cited(&promoted), ["decision:cache-port"]);
    // Confidence stays the words' own: this one barely shares a word.
    assert_eq!(confidence(&promoted), MemoryConfidence::Low);
    assert_eq!(marked(&promoted, "judged_by"), ["decision:cache-port"]);
}

#[test]
fn an_unanchored_answer_loses_only_what_the_judge_vetoed() {
    let question = "Where does the cache server listen and where did it move?";
    let plain = answer(context(RUNBOOK), question);
    assert_eq!(status(&plain), AnswerStatus::Answered);
    let before = cited(&plain).len();
    assert!(before >= 2, "{:?}", cited(&plain));

    let mut context = context(RUNBOOK);
    let band = band_of(&mut context, question, ALWAYS).expect("in the band");
    assert_eq!(band.entry, DoubtEntry::NarrowMargin);
    let response = answer(
        context.with_doubt_verdicts(judged(&band, false, |passage| {
            if names(passage, "rack") { 100 } else { 700 }
        })),
        question,
    );
    assert_eq!(status(&response), AnswerStatus::Answered);
    assert!(!cited(&response).contains(&"observation:cache-rack"));
    assert_eq!(cited(&response).len(), before - 1);
    assert!(
        crate::v1beta1::memory_mapping::judged_core::at_most(
            confidence(&response),
            confidence(&plain)
        ) == confidence(&response),
        "a veto never raises confidence"
    );
}
