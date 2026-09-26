use kmp_proto::v1beta1::{AnswerStatus, MemoryEvidence, UnknownReason};

use super::answer_candidate_terms::AnswerCandidateTerms;
use super::answer_ranker::ANSWER_CORE_LIMIT;
use super::gate_verdict::GateVerdict;
use super::question_anchor::QuestionAnchor;
use super::question_contract::QuestionContract;
use super::question_form::QuestionForm;

/// The anchored decision of `evidence_or_unknown` and `show_conflicts`.
///
/// The ⌈2/3⌉ rule counts concepts without knowing which of them matter, so
/// an entry about `#188` answered a question about `#288` with the rest of
/// its words. A question that names an identifier is about that identifier:
/// the gate cites only memories that name its rarest required anchor
/// literally, and none that names an anchor the question excluded.
///
/// What it asks of the anchor must stand beside it. A singular question is
/// answered when one cited memory states at least ⌈2/3⌉ of its subject — the
/// same bar as the ungated rule, now over the concepts that are not the
/// anchor, a facet or a word about asking, and inside one memory that names
/// the anchor. An enumerative question is answered when every concept of its
/// subject stands in some cited memory, and PARTIAL, what was found cited and
/// what was not named, when only some do. Otherwise it is UNKNOWN,
/// `attribute_not_found`, naming what the best memory lacks.
pub(super) struct AnchoredGate<'a> {
    contract: &'a QuestionContract,
    allow_partial: bool,
}

impl<'a> AnchoredGate<'a> {
    pub(super) fn new(contract: &'a QuestionContract, allow_partial: bool) -> Self {
        Self {
            contract,
            allow_partial,
        }
    }

    /// Decides over the candidates the question reached in its own words, in
    /// rank order. `answers(key, terms)` is the same match the focus rule
    /// makes: stem, concept key or the lexical bridge.
    pub(super) fn decide<'e>(
        &self,
        principal: &QuestionAnchor,
        others: &[QuestionAnchor],
        ranked: impl IntoIterator<Item = (&'e MemoryEvidence, AnswerCandidateTerms)>,
        answers: impl Fn(&str, &AnswerCandidateTerms) -> bool,
    ) -> GateVerdict {
        let names = |terms: &AnswerCandidateTerms, term: &str| terms.direct_counts.count(term) > 0;
        let negated = self.contract.negated_terms();
        let core = ranked
            .into_iter()
            .filter(|(_, terms)| names(terms, &principal.term))
            .filter(|(_, terms)| !negated.iter().any(|term| names(terms, term)))
            .take(ANSWER_CORE_LIMIT)
            .collect::<Vec<_>>();
        if core.is_empty() {
            return GateVerdict::unknown(UnknownReason::NoBearing, Vec::new());
        }
        let subject = self.contract.subject();
        let stated = |terms: &AnswerCandidateTerms| {
            subject
                .iter()
                .map(|(key, _)| answers(key, terms))
                .collect::<Vec<_>>()
        };
        let per_item = core
            .iter()
            .map(|(_, terms)| stated(terms))
            .collect::<Vec<_>>();
        let mut missing = others
            .iter()
            .filter(|anchor| !core.iter().any(|(_, terms)| names(terms, &anchor.term)))
            .map(|anchor| anchor.written.clone())
            .collect::<Vec<_>>();
        let enumerative = self.contract.form() == QuestionForm::Enumerative;
        let found = if enumerative {
            // Each concept in some cited memory.
            (0..subject.len())
                .map(|index| per_item.iter().any(|item| item[index]))
                .collect::<Vec<_>>()
        } else {
            // The memory that states most of it, first in rank on a tie.
            per_item
                .iter()
                .enumerate()
                .max_by_key(|(position, item)| {
                    (
                        item.iter().filter(|stated| **stated).count(),
                        std::cmp::Reverse(*position),
                    )
                })
                .map(|(_, item)| item.clone())
                .unwrap_or_default()
        };
        let stated_count = found.iter().filter(|stated| **stated).count();
        let required = if enumerative {
            subject.len()
        } else {
            (subject.len() * 2).div_ceil(3)
        };
        let subject_missing = subject
            .iter()
            .zip(&found)
            .filter(|(_, stated)| !**stated)
            .map(|((_, word), _)| word.clone())
            .collect::<Vec<_>>();
        let cited = core.iter().map(|(item, _)| item.id.clone()).collect();
        if missing.is_empty() && stated_count >= required {
            return GateVerdict {
                status: AnswerStatus::Answered,
                reason: UnknownReason::Unspecified,
                core: cited,
                missing,
            };
        }
        missing.extend(subject_missing);
        if self.allow_partial && enumerative {
            return GateVerdict {
                status: AnswerStatus::Partial,
                reason: UnknownReason::Unspecified,
                core: cited,
                missing,
            };
        }
        GateVerdict::unknown(UnknownReason::AttributeNotFound, missing)
    }
}
