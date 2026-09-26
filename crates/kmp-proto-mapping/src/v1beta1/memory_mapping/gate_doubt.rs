use std::collections::{BTreeMap, BTreeSet};

use kmp_proto::v1beta1::{AnswerStatus, MemoryEvidence, UnknownReason};

use super::answer_ranker::ANSWER_CORE_LIMIT;
use super::doubt_verdicts::DoubtVerdicts;
use super::gate_verdict::GateVerdict;

/// The anchored gate's verdict after a doubt band's judge (DESIGN L4 4f).
///
/// B1 takes vetoed citations out and asks the gate again over what is left
/// of the core — never over anything the core did not cite, so a veto can
/// only take an answer away. B2, when the store allows it, adds memories
/// that name the principal anchor and that the judge finds answer: after
/// the lexical citations of an answer, or as the answer to an UNKNOWN whose
/// anchor was found.
pub(super) struct GateDoubt<'d> {
    pub(super) verdict: GateVerdict,
    doubt: Option<&'d DoubtVerdicts>,
    vetoed: BTreeMap<String, u16>,
    promoted: BTreeMap<String, u16>,
}

impl<'d> GateDoubt<'d> {
    /// `promotable` are the memories the gate may cite, in rank order;
    /// `redecide(kept)` is the gate over only the ids in `kept`.
    pub(super) fn judge(
        verdict: GateVerdict,
        doubt: Option<&'d DoubtVerdicts>,
        promotable: &[&MemoryEvidence],
        redecide: impl Fn(&BTreeSet<String>) -> GateVerdict,
    ) -> Self {
        let Some(doubt) = doubt else {
            return Self::unjudged(verdict);
        };
        if verdict.reason == UnknownReason::AnchorAbsentInSelection {
            return Self::unjudged(verdict);
        }
        let by_id = promotable
            .iter()
            .map(|item| (item.id.as_str(), *item))
            .collect::<BTreeMap<_, _>>();
        let vetoed = verdict
            .core
            .iter()
            .filter_map(|id| {
                let item = by_id.get(id.as_str())?;
                doubt.vetoes(item).map(|permille| (id.clone(), permille))
            })
            .collect::<BTreeMap<_, _>>();
        let mut verdict = if vetoed.is_empty() {
            verdict
        } else {
            let kept = verdict
                .core
                .iter()
                .filter(|id| !vetoed.contains_key(*id))
                .cloned()
                .collect::<BTreeSet<_>>();
            redecide(&kept)
        };
        let room = match verdict.status {
            AnswerStatus::Unknown => ANSWER_CORE_LIMIT,
            _ => ANSWER_CORE_LIMIT.saturating_sub(verdict.core.len()),
        };
        let promoted = promotable
            .iter()
            .filter(|item| {
                !verdict.core.contains(&item.id) && !vetoed.contains_key(item.id.as_str())
            })
            .filter_map(|item| {
                doubt
                    .promotes(item)
                    .map(|permille| (item.id.clone(), permille))
            })
            .take(room)
            .collect::<Vec<_>>();
        if !promoted.is_empty() {
            let ids = promoted.iter().map(|(id, _)| id.clone());
            if verdict.status == AnswerStatus::Unknown {
                verdict = GateVerdict {
                    status: AnswerStatus::Answered,
                    reason: UnknownReason::Unspecified,
                    core: ids.collect(),
                    missing: Vec::new(),
                };
            } else {
                verdict.core.extend(ids);
            }
        }
        Self {
            verdict,
            doubt: Some(doubt),
            vetoed,
            promoted: promoted.into_iter().collect(),
        }
    }

    fn unjudged(verdict: GateVerdict) -> Self {
        Self {
            verdict,
            doubt: None,
            vetoed: BTreeMap::new(),
            promoted: BTreeMap::new(),
        }
    }

    /// The memory with the judge's mark, if the judge moved it.
    pub(super) fn mark(&self, item: MemoryEvidence) -> MemoryEvidence {
        let Some(doubt) = self.doubt else {
            return item;
        };
        if let Some(permille) = self.vetoed.get(&item.id) {
            let permille = *permille;
            return doubt.mark_out(item, permille);
        }
        if let Some(permille) = self.promoted.get(&item.id) {
            let permille = *permille;
            return doubt.mark_in(item, permille);
        }
        item
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::v1beta1::memory_mapping::doubt_entry::DoubtEntry;
    use crate::v1beta1::memory_mapping::doubt_judgement::DoubtJudgement;

    fn item(id: &str) -> MemoryEvidence {
        MemoryEvidence {
            id: id.into(),
            text: format!("text of {id}"),
            ..MemoryEvidence::default()
        }
    }

    fn doubt(promote_at: Option<u16>, judged: &[(&str, u16)]) -> DoubtVerdicts {
        let mut verdicts = DoubtVerdicts::new(
            DoubtEntry::Partial,
            "jev".into(),
            "doubt_band.v1".into(),
            800,
            promote_at,
        )
        .expect("valid");
        for (id, answers) in judged {
            let sha = format!("{:x}", Sha256::digest(format!("text of {id}").as_bytes()));
            verdicts
                .judge(
                    id,
                    &sha,
                    DoubtJudgement {
                        answers: *answers,
                        not_answers: 1000 - answers,
                    },
                )
                .expect("judged");
        }
        verdicts
    }

    fn answered(core: &[&str]) -> GateVerdict {
        GateVerdict {
            status: AnswerStatus::Answered,
            reason: UnknownReason::Unspecified,
            core: core.iter().map(|id| id.to_string()).collect(),
            missing: Vec::new(),
        }
    }

    #[test]
    fn a_veto_asks_the_gate_again_over_what_is_left_of_the_core() {
        let items = [item("a"), item("b"), item("c")];
        let promotable = items.iter().collect::<Vec<_>>();
        let doubt = doubt(None, &[("a", 50), ("b", 900), ("c", 990)]);
        let asked = std::cell::RefCell::new(Vec::new());
        let judged = GateDoubt::judge(answered(&["a", "b"]), Some(&doubt), &promotable, |kept| {
            asked.borrow_mut().push(kept.clone());
            answered(&kept.iter().map(String::as_str).collect::<Vec<_>>())
        });
        assert_eq!(
            asked.into_inner(),
            vec![BTreeSet::from(["b".to_string()])],
            "c was never cited, so it cannot enter through a veto"
        );
        assert_eq!(judged.verdict.core, vec!["b"]);
        let marked = judged.mark(item("a"));
        assert_eq!(marked.metadata["judged_out"], "jev");
        assert_eq!(marked.metadata["judged_permille"], "950");
        assert!(judged.mark(item("b")).metadata.is_empty());
    }

    #[test]
    fn a_vetoed_core_left_empty_is_unknown() {
        let items = [item("a")];
        let promotable = items.iter().collect::<Vec<_>>();
        let doubt = doubt(None, &[("a", 100)]);
        let judged = GateDoubt::judge(answered(&["a"]), Some(&doubt), &promotable, |kept| {
            assert!(kept.is_empty());
            GateVerdict::unknown(UnknownReason::NoBearing, Vec::new())
        });
        assert_eq!(judged.verdict.status, AnswerStatus::Unknown);
    }

    #[test]
    fn a_promotion_answers_an_attribute_not_found_and_follows_an_answer() {
        let items = [item("a"), item("b"), item("c")];
        let promotable = items.iter().collect::<Vec<_>>();
        let doubt = doubt(Some(900), &[("a", 500), ("b", 950), ("c", 990)]);
        let unknown = GateVerdict::unknown(UnknownReason::AttributeNotFound, vec!["port".into()]);
        let judged = GateDoubt::judge(unknown, Some(&doubt), &promotable, |_| {
            panic!("nothing was vetoed")
        });
        assert_eq!(judged.verdict.status, AnswerStatus::Answered);
        assert_eq!(judged.verdict.core, vec!["b", "c"]);
        assert!(judged.verdict.missing.is_empty());
        assert_eq!(judged.mark(item("c")).metadata["judged_by"], "jev");
        let judged = GateDoubt::judge(answered(&["a"]), Some(&doubt), &promotable, |_| {
            panic!("nothing was vetoed")
        });
        assert_eq!(judged.verdict.core, vec!["a", "b", "c"]);
    }

    #[test]
    fn without_a_judge_or_with_an_absent_anchor_nothing_moves() {
        let items = [item("a")];
        let promotable = items.iter().collect::<Vec<_>>();
        let judged = GateDoubt::judge(answered(&["a"]), None, &promotable, |_| panic!("no judge"));
        assert_eq!(judged.verdict.core, vec!["a"]);
        assert!(judged.mark(item("a")).metadata.is_empty());
        let doubt = doubt(Some(900), &[("a", 990)]);
        let absent =
            GateVerdict::unknown(UnknownReason::AnchorAbsentInSelection, vec!["#9".into()]);
        let judged = GateDoubt::judge(absent.clone(), Some(&doubt), &promotable, |_| panic!("no"));
        assert_eq!(judged.verdict, absent);
    }
}
