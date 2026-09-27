use std::collections::BTreeMap;

use kmp_proto::v1beta1::{MemoryConfidence, MemoryEvidence};

use super::answer_ranker::ANSWER_CORE_LIMIT;
use super::answer_selection::was_reached_indirectly;
use super::ask_setup::AskSetup;
use super::doubt_verdicts::DoubtVerdicts;
use super::judged_core::JudgedCore;

/// A doubt band's judge on the core of a question no anchor decided.
///
/// B1: a cited memory the judge vetoes leaves the core and stays in the
/// proof, marked; the rest of the core is what the deterministic reading
/// cited, so nothing it left out slides in. B2, when the store allows it: an
/// admitted, live memory the question reached directly, that names no
/// excluded anchor alone and that the judge finds answers, joins the core,
/// marked, up to the core's limit.
///
/// Returns the proof, the core and what the judge did.
pub(super) fn apply_unanchored_doubt(
    doubt: &DoubtVerdicts,
    setup: &AskSetup<'_>,
    asked: &str,
    mut evidence: Vec<MemoryEvidence>,
    answer_core: Vec<MemoryEvidence>,
    judged_pool: Vec<MemoryEvidence>,
) -> (Vec<MemoryEvidence>, Vec<MemoryEvidence>, Option<JudgedCore>) {
    let ranker = &setup.ranker;
    let cited = &answer_core[..answer_core.len().min(ANSWER_CORE_LIMIT)];
    let confidence = ranker.confidence(asked, cited);
    let bore = !cited.is_empty() && confidence != MemoryConfidence::Low;
    let vetoed = cited
        .iter()
        .filter_map(|item| {
            doubt
                .vetoes(item)
                .map(|permille| (item.id.clone(), permille))
        })
        .collect::<BTreeMap<_, _>>();
    let kept = cited
        .iter()
        .filter(|item| !vetoed.contains_key(&item.id))
        .cloned()
        .collect::<Vec<_>>();
    let room = ANSWER_CORE_LIMIT - kept.len();
    let promoted = evidence
        .iter()
        .chain(judged_pool.iter())
        .filter(|item| {
            !kept.iter().any(|core| core.id == item.id)
                && !vetoed.contains_key(&item.id)
                && !was_reached_indirectly(item)
                && ranker.is_live(item)
                && !setup.negated_only(item)
        })
        .filter_map(|item| {
            doubt
                .promotes(item)
                .map(|permille| (item.id.clone(), permille))
        })
        .fold(Vec::<(String, u16)>::new(), |mut found, (id, permille)| {
            if !found.iter().any(|(seen, _)| *seen == id) {
                found.push((id, permille));
            }
            found
        })
        .into_iter()
        .take(room)
        .collect::<BTreeMap<_, _>>();
    let judged = JudgedCore {
        bore,
        confidence,
        kept: !kept.is_empty(),
        promoted: !promoted.is_empty(),
    };
    // Marks go on the proof's own copies; a promoted memory the proof did
    // not carry joins it after the core it stands in.
    for item in &mut evidence {
        if let Some(permille) = vetoed.get(&item.id) {
            *item = doubt.mark_out(std::mem::take(item), *permille);
        } else if let Some(permille) = promoted.get(&item.id) {
            *item = doubt.mark_in(std::mem::take(item), *permille);
        }
    }
    let mut joined = Vec::new();
    for item in judged_pool {
        if let Some(permille) = promoted.get(&item.id)
            && !evidence.iter().any(|carried| carried.id == item.id)
            && !joined
                .iter()
                .any(|carried: &MemoryEvidence| carried.id == item.id)
        {
            joined.push(doubt.mark_in(item, *permille));
        }
    }
    let after_core = evidence
        .iter()
        .rposition(|item| kept.iter().any(|core| core.id == item.id))
        .map_or(0, |position| position + 1);
    evidence.splice(after_core..after_core, joined);
    let promoted_core = evidence
        .iter()
        .filter(|item| promoted.contains_key(&item.id))
        .cloned();
    let core = kept
        .iter()
        .map(|item| {
            evidence
                .iter()
                .find(|carried| carried.id == item.id)
                .cloned()
                .unwrap_or_else(|| item.clone())
        })
        .chain(promoted_core)
        .collect::<Vec<_>>();
    (evidence, core, Some(judged))
}
