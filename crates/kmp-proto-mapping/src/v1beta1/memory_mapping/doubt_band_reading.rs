use std::collections::BTreeMap;

use kmp_application::MemoryAnswerPolicy;
use kmp_domain::TemporalSelection;
use kmp_proto::v1beta1::{AnswerStatus, MemoryConfidence, MemoryEvidence, UnknownReason};
use sha2::{Digest, Sha256};

use super::answer_ranker::ANSWER_CORE_LIMIT;
use super::answer_selection::was_reached_indirectly;
use super::ask_retrieval_context::AskRetrievalContext;
use super::ask_setup::AskSetup;
use super::content_scores::ContentScores;
use super::decided_selection::DecidedSelection;
use super::doubt_band::{DoubtBand, MAX_DOUBT_PASSAGES};
use super::doubt_entry::DoubtEntry;
use super::doubt_passage::DoubtPassage;
use super::gate_verdict::GateVerdict;
use super::lexical_bridge::LexicalBridge;
use super::ranked_selection::RankedSelection;
use super::scalars::ProtoMappingResult;

/// What a doubt band read: the band, if the ask is in it, and the reading
/// the answer may reuse — the anchored gate's, or the plain ranking.
pub(super) type DoubtReading = (
    Option<DoubtBand>,
    Option<DecidedSelection>,
    Option<RankedSelection>,
);

/// Reads the question as the answer will (the same setup, ranking and
/// gate) and says whether it falls in the doubt band (DESIGN L4 4f):
///
/// - no required anchor is absent, and
/// - (i) no anchor decided, the reading is UNKNOWN and a `best_effort`
///   reading would cite something; or (ii) the anchored gate found the
///   anchor but not what was asked (`attribute_not_found`), or only part of
///   it (PARTIAL); or (iii) it answered with a first citation leading the
///   second by less than `margin_below` tenths.
///
/// The passages are the core first, then the memories that could be cited
/// in rank order: under an anchor, direct candidates naming the principal
/// anchor (or standing in for it by a declared `same_entity_as`); without
/// one, direct, live candidates that name no excluded anchor alone.
pub(super) fn read_doubt_band(
    retrieval: &AskRetrievalContext,
    question: &str,
    policy: MemoryAnswerPolicy,
    temporal: &TemporalSelection,
    bridge: &LexicalBridge,
    margin_below: i64,
) -> ProtoMappingResult<DoubtReading> {
    let Some(gate) = retrieval.gate else {
        return Ok((None, None, None));
    };
    if !AskSetup::policy_is_strict(policy) {
        return Ok((None, None, None));
    }
    let setup = AskSetup::read(
        &retrieval.result,
        question,
        temporal,
        bridge,
        retrieval.lexical_cache.as_deref(),
        true,
    )?;
    let asked = setup.asked(question);
    let candidates = setup.candidate_evidence.clone();
    let anchored = setup
        .contract
        .as_ref()
        .filter(|contract| contract.requires_anchors());
    if let Some(contract) = anchored {
        let reading = setup.ranker.read_anchored(
            asked,
            contract.anchored_asked().unwrap_or(question),
            policy,
            contract,
            gate,
            candidates,
            None,
        );
        let band = match &reading.verdict {
            Some(verdict) => anchored_band(
                verdict,
                &reading.evidence,
                &reading.scores,
                &reading.promotable,
                margin_below,
            ),
            None => unanchored_band(
                &setup,
                asked,
                &reading.evidence,
                &reading.scores,
                margin_below,
            ),
        };
        let decided = DecidedSelection::new(
            question,
            policy,
            temporal,
            bridge,
            reading.evidence,
            reading.verdict,
        );
        return Ok((band, Some(decided), None));
    }
    let (ranked, scores) = setup.ranker.rank_scored(asked, policy, candidates);
    let band = unanchored_band(&setup, asked, &ranked, &scores, margin_below);
    let ranked = RankedSelection::new(asked, policy, temporal, bridge, ranked);
    Ok((band, None, Some(ranked)))
}

fn anchored_band(
    verdict: &GateVerdict,
    evidence: &[MemoryEvidence],
    scores: &ContentScores,
    promotable: &[String],
    margin_below: i64,
) -> Option<DoubtBand> {
    let margin = (verdict.status == AnswerStatus::Answered)
        .then(|| scores.margin(verdict.core.iter().map(String::as_str)));
    let entry = match (verdict.status, verdict.reason) {
        (AnswerStatus::Unknown, UnknownReason::AttributeNotFound) => DoubtEntry::AttributeNotFound,
        (AnswerStatus::Partial, _) => DoubtEntry::Partial,
        (AnswerStatus::Answered, _) if margin.is_some_and(|margin| margin < margin_below) => {
            DoubtEntry::NarrowMargin
        }
        _ => return None,
    };
    let by_id = evidence
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let cited = verdict.cited();
    let passages = verdict
        .core
        .iter()
        .chain(promotable.iter().filter(|id| !cited.contains(id.as_str())))
        .filter_map(|id| by_id.get(id.as_str()))
        .take(MAX_DOUBT_PASSAGES)
        .map(|item| passage(item, cited.contains(item.id.as_str())))
        .collect::<Vec<_>>();
    (!passages.is_empty()).then_some(DoubtBand {
        entry,
        margin,
        passages,
    })
}

fn unanchored_band(
    setup: &AskSetup<'_>,
    asked: &str,
    ranked: &[MemoryEvidence],
    scores: &ContentScores,
    margin_below: i64,
) -> Option<DoubtBand> {
    let citable = |item: &&MemoryEvidence| {
        !was_reached_indirectly(item) && !setup.negated_only(item) && setup.ranker.is_live(item)
    };
    let direct = ranked.iter().filter(citable).collect::<Vec<_>>();
    // The core the answer would cite: direct and not excluded, as the
    // answer reads it, before liveness (a history question cites history).
    let core = ranked
        .iter()
        .filter(|item| !was_reached_indirectly(item) && !setup.negated_only(item))
        .take(ANSWER_CORE_LIMIT)
        .cloned()
        .collect::<Vec<_>>();
    let confidence = setup.ranker.confidence(asked, &core);
    if !core.is_empty() && confidence != MemoryConfidence::Low {
        let margin = scores.margin(core.iter().map(|item| item.id.as_str()));
        if margin >= margin_below {
            return None;
        }
        let passages = core
            .iter()
            .map(|item| passage(item, true))
            .chain(
                direct
                    .iter()
                    .filter(|item| !core.iter().any(|cited| cited.id == item.id))
                    .map(|item| passage(item, false)),
            )
            .take(MAX_DOUBT_PASSAGES)
            .collect::<Vec<_>>();
        return Some(DoubtBand {
            entry: DoubtEntry::NarrowMargin,
            margin: Some(margin),
            passages,
        });
    }
    let best_effort = setup.ranker.rank(
        asked,
        MemoryAnswerPolicy::BestEffort,
        setup.candidate_evidence.clone(),
    );
    let passages = best_effort
        .iter()
        .filter(citable)
        .take(MAX_DOUBT_PASSAGES)
        .map(|item| passage(item, false))
        .collect::<Vec<_>>();
    (!passages.is_empty()).then_some(DoubtBand {
        entry: DoubtEntry::UnanchoredUnknown,
        margin: None,
        passages,
    })
}

fn passage(item: &MemoryEvidence, in_core: bool) -> DoubtPassage {
    DoubtPassage {
        id: item.id.clone(),
        entry_ref: item
            .supports
            .first()
            .cloned()
            .unwrap_or_else(|| item.id.clone()),
        text: item.text.clone(),
        text_sha256: format!("{:x}", Sha256::digest(item.text.as_bytes())),
        in_core,
        promotable: true,
    }
}
