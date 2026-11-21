//! The cross-language judge's two ways into the doubt band
//! (`ask-cross-language.json`, DESIGN L4 4f): an answer that stands only
//! through the lexical bridge's translation, which the judge may veto, and an
//! UNKNOWN the bridge reached memories in another language for, which the
//! judge may rescue.

use std::collections::BTreeMap;

use kmp_proto::v1beta1::{AnswerStatus, MemoryEvidence};

use super::answer_selection::{BRIDGED_TERMS_KEY, REACHED_BY_BRIDGE, REACHED_BY_KEY};
use super::ask_setup::AskSetup;
use super::cross_language::CrossLanguage;
use super::doubt_band::{DoubtBand, MAX_DOUBT_PASSAGES};
use super::doubt_band_reading::passage;
use super::doubt_entry::DoubtEntry;
use super::gate_verdict::GateVerdict;

/// An anchored answer the gate gave only because the table translated what
/// its citations state (`GateVerdict::translated`): its cited passages.
pub(super) fn translated_answer_band(
    verdict: &GateVerdict,
    evidence: &[MemoryEvidence],
) -> Option<DoubtBand> {
    if !verdict.translated
        || !matches!(
            verdict.status,
            AnswerStatus::Answered | AnswerStatus::Partial
        )
    {
        return None;
    }
    let by_id = evidence
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    cited_band(
        verdict
            .core
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied()),
    )
}

/// An unanchored answer every citation of which is written in another
/// language than the question and was carried by the table's words
/// (`bridged_terms`): in the reader's own words none of them would have
/// answered.
pub(super) fn translated_core_band(question: &str, core: &[MemoryEvidence]) -> Option<DoubtBand> {
    let crossing = CrossLanguage::of_question(question);
    let translated = !core.is_empty()
        && core.iter().all(|item| {
            item.metadata.contains_key(BRIDGED_TERMS_KEY) && crossing.crosses_to(&item.text)
        });
    if !translated {
        return None;
    }
    cited_band(core.iter())
}

fn cited_band<'e>(cited: impl Iterator<Item = &'e MemoryEvidence>) -> Option<DoubtBand> {
    let passages = cited
        .take(MAX_DOUBT_PASSAGES)
        .map(|item| passage(item, true))
        .collect::<Vec<_>>();
    (!passages.is_empty()).then_some(DoubtBand {
        entry: DoubtEntry::TranslatedAnswer,
        margin: None,
        passages,
    })
}

/// An unanchored UNKNOWN's candidates in another language: memories the
/// table reached (marked `reached_by: bridge`, or carrying `bridged_terms`)
/// whose text leans in another language than the question, live and naming
/// no excluded anchor alone; the strict reading's order first, then the
/// `best_effort` one, at most eight.
pub(super) fn rescue_band(
    setup: &AskSetup<'_>,
    question: &str,
    ranked: &[MemoryEvidence],
    best_effort: &[MemoryEvidence],
) -> Option<DoubtBand> {
    let crossing = CrossLanguage::of_question(question);
    let mut seen = Vec::<&str>::new();
    let passages = ranked
        .iter()
        .chain(best_effort)
        .filter(|item| {
            bridged(item)
                && crossing.crosses_to(&item.text)
                && setup.ranker.is_live(item)
                && !setup.negated_only(item)
        })
        .filter(|item| {
            let fresh = !seen.contains(&item.id.as_str());
            if fresh {
                seen.push(item.id.as_str());
            }
            fresh
        })
        .take(MAX_DOUBT_PASSAGES)
        .map(|item| passage(item, false))
        .collect::<Vec<_>>();
    (!passages.is_empty()).then_some(DoubtBand {
        entry: DoubtEntry::CrossLanguageUnknown,
        margin: None,
        passages,
    })
}

/// Whether the table touched this candidate: it reached it, or it carried
/// some of the words it answered with.
pub(super) fn bridged(item: &MemoryEvidence) -> bool {
    item.metadata
        .get(REACHED_BY_KEY)
        .is_some_and(|how| how == REACHED_BY_BRIDGE)
        || item.metadata.contains_key(BRIDGED_TERMS_KEY)
}
