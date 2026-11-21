//! The cross-language judge's entries into the doubt band
//! (`ask-cross-language.json`), through `AskRetrievalContext::doubt_band`
//! and `ask_response_from_result` as the MCP ask reads them: the band, the
//! judge's verdicts, then the answer.
use kmp_application::MemoryAnswerPolicy;
use kmp_domain::TemporalSelection;
use kmp_proto::v1beta1::{AnswerStatus, AskResponse, MemoryConfidence};

use super::anchored_gate_tests::{cited, confidence, status, store_related};
use super::doubt_band::DoubtBand;
use super::doubt_band_entries::DoubtBandEntries;
use super::doubt_entry::DoubtEntry;
use super::doubt_judgement::DoubtJudgement;
use super::doubt_verdicts::DoubtVerdicts;
use super::lexical_bridge::tests::table;
use super::{AskGate, AskRetrievalContext, LexicalBridge, ask_response_from_result};

/// `deadline` and its Spanish `plazo`; `night` and `noche`.
fn toy() -> LexicalBridge {
    LexicalBridge::from_bytes(&table(
        "toy",
        &[
            ("deadline", &[127, 0, 0, 0]),
            ("plazo", &[120, 30, 0, 0]),
            ("night", &[0, 127, 0, 0]),
            ("noche", &[0, 120, 30, 0]),
        ],
    ))
    .expect("the toy table parses")
}

const PLAZO: &[(&str, &str, &str)] = &[(
    "decision:plazo",
    "decision",
    "#10132 fija el plazo de svc-013 en eu-west-1 para el viernes.",
)];
const TRANSLATED: &str = "What is the deadline of #10132 for svc-013 in eu-west-1?";

const LEAK: &[(&str, &str, &str)] = &[
    (
        "observation:fuga",
        "observation",
        "El turno de noche encontró una fuga debajo de Ostrova.",
    ),
    (
        "observation:roof",
        "observation",
        "The roof crew painted the north wall on Monday.",
    ),
];
const RESCUED: &str = "Where did the night shift find a leak?";

fn context(entries: &[(&str, &str, &str)]) -> AskRetrievalContext {
    AskRetrievalContext::from(store_related(entries, &[])).with_gate(AskGate::anchored(true))
}

fn band(
    context: &mut AskRetrievalContext,
    question: &str,
    entries: DoubtBandEntries,
) -> Option<DoubtBand> {
    context
        .doubt_band(
            question,
            MemoryAnswerPolicy::EvidenceOrUnknown,
            &TemporalSelection::Frontier,
            &toy(),
            entries,
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
        &toy(),
        &TemporalSelection::Frontier,
    )
    .expect("an ask response")
}

/// Verdicts on every passage: each judged `states` thousandths likely to
/// state the answer; a translated citation kept at 700; a rescue at 900
/// with `companion`.
fn verdicts(band: &DoubtBand, states: u16, companion: Option<u16>) -> DoubtVerdicts {
    let mut verdicts = DoubtVerdicts::new(
        band.entry,
        "jev-test".into(),
        "cross_language.v1".into(),
        500,
        None,
    )
    .expect("thresholds");
    if let Some(companion) = companion {
        verdicts = verdicts.with_rescue(900, companion).expect("a rescue");
    }
    if band.entry == DoubtEntry::TranslatedAnswer {
        verdicts = verdicts.with_keep_line(700).expect("a veto");
    }
    for passage in &band.passages {
        verdicts
            .judge(
                &passage.id,
                &passage.text_sha256,
                DoubtJudgement {
                    answers: states,
                    not_answers: 1_000 - states,
                },
            )
            .expect("judged");
    }
    verdicts
}

fn veto_only() -> DoubtBandEntries {
    DoubtBandEntries::default().with_cross_language(true, false)
}

fn rescue_only() -> DoubtBandEntries {
    DoubtBandEntries::default().with_cross_language(false, true)
}

#[test]
fn closed_entries_read_no_band() {
    let mut context = context(PLAZO);
    assert_eq!(
        band(&mut context, TRANSLATED, DoubtBandEntries::default()),
        None
    );
    let mut context = self::context(LEAK);
    assert_eq!(
        band(&mut context, RESCUED, veto_only()),
        None,
        "an UNKNOWN has nothing to veto"
    );
}

#[test]
fn a_translated_answer_enters_with_its_citation_and_a_veto_takes_it_away() {
    let mut context = context(PLAZO);
    let band = band(&mut context, TRANSLATED, veto_only()).expect("in the band");
    assert_eq!(band.entry, DoubtEntry::TranslatedAnswer);
    assert_eq!(band.passages.len(), 1);
    assert!(band.passages[0].in_core);

    let vetoed = answer(
        context.with_doubt_verdicts(verdicts(&band, 100, None)),
        TRANSLATED,
    );
    assert_eq!(status(&vetoed), AnswerStatus::Unknown, "{vetoed:?}");
    assert!(cited(&vetoed).is_empty());

    // Unsettled is not a yes: a translation it does not vouch for leaves.
    let mut context = self::context(PLAZO);
    let band = self::band(&mut context, TRANSLATED, veto_only()).expect("in the band");
    let unsettled = answer(
        context.with_doubt_verdicts(verdicts(&band, 550, None)),
        TRANSLATED,
    );
    assert_eq!(status(&unsettled), AnswerStatus::Unknown, "{unsettled:?}");

    let mut context = self::context(PLAZO);
    let band = self::band(&mut context, TRANSLATED, veto_only()).expect("in the band");
    let kept = answer(
        context.with_doubt_verdicts(verdicts(&band, 900, None)),
        TRANSLATED,
    );
    assert_eq!(status(&kept), AnswerStatus::Answered);
    assert_eq!(cited(&kept), ["decision:plazo"]);
    assert_eq!(confidence(&kept), MemoryConfidence::Medium);
}

#[test]
fn an_unknown_the_bridge_reached_another_language_for_is_rescued_on_both_judgements() {
    let unjudged = answer(context(LEAK), RESCUED);
    assert_eq!(status(&unjudged), AnswerStatus::Unknown, "{unjudged:?}");

    let mut context = self::context(LEAK);
    let band = band(&mut context, RESCUED, rescue_only()).expect("in the band");
    assert_eq!(band.entry, DoubtEntry::CrossLanguageUnknown);
    assert_eq!(
        band.passages
            .iter()
            .map(|p| p.entry_ref.as_str())
            .collect::<Vec<_>>(),
        ["observation:fuga"],
        "only memories in another language that the table reached"
    );
    let rescued = answer(
        context.with_doubt_verdicts(verdicts(&band, 970, Some(950))),
        RESCUED,
    );
    assert_eq!(status(&rescued), AnswerStatus::Answered, "{rescued:?}");
    assert_eq!(cited(&rescued), ["observation:fuga"]);
    assert_ne!(confidence(&rescued), MemoryConfidence::High);
    let evidence = &rescued.proof.as_ref().expect("proof").evidence;
    let cited_memory = evidence
        .iter()
        .find(|item| item.supports.first().map(String::as_str) == Some("observation:fuga"))
        .expect("the rescued memory is in the proof");
    assert_eq!(cited_memory.metadata["judged_by"], "jev-test");
    assert_eq!(
        cited_memory.metadata["judged_template"],
        "cross_language.v1"
    );
    assert_eq!(
        cited_memory.text, LEAK[0].2,
        "the cited text is the stored text"
    );
    assert!(
        rescued
            .summary
            .contains("jev-test's judgement that it states the answer in another language"),
        "{}",
        rescued.summary
    );
}

#[test]
fn a_doubtful_companion_or_passage_rescues_nothing() {
    for (states, companion) in [(970, 600), (700, 990)] {
        let mut context = context(LEAK);
        let band = band(&mut context, RESCUED, rescue_only()).expect("in the band");
        let response = answer(
            context.with_doubt_verdicts(verdicts(&band, states, Some(companion))),
            RESCUED,
        );
        assert_eq!(
            status(&response),
            AnswerStatus::Unknown,
            "{states}/{companion}"
        );
        assert!(response.because.is_empty());
    }
}

#[test]
fn a_rescue_needs_its_own_entry_and_no_promotion_bar() {
    let band = DoubtBand {
        entry: DoubtEntry::NarrowMargin,
        margin: Some(1),
        passages: Vec::new(),
    };
    assert!(verdicts(&band, 900, None).with_rescue(900, 950).is_err());
    let promoting = DoubtVerdicts::new(
        DoubtEntry::CrossLanguageUnknown,
        "m".into(),
        "t.v1".into(),
        500,
        Some(900),
    )
    .expect("valid");
    assert!(promoting.with_rescue(900, 950).is_err());
}
