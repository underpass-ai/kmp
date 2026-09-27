use std::borrow::Cow;
use std::collections::BTreeSet;

use kmp_application::{GetContextResult, MemoryAnswerPolicy};
use kmp_domain::{KmpBundle, TemporalSelection};
use kmp_proto::v1beta1::MemoryEvidence;

use super::answer_ranker::AnswerEvidenceRanker;
use super::bundle_views::answer_evidence_from_bundle;
use super::lexical_bridge::LexicalBridge;
use super::lexical_index_cache::LexicalIndexCache;
use super::lexical_index_identity::LexicalIndexIdentity;
use super::question_contract::QuestionContract;
use super::responses::lifecycle_for;
use super::scalars::ProtoMappingResult;
use super::temporal_admission::TemporalAdmission;

/// What an ask reads before it ranks a word: the clock's admission, the
/// bounded bundle and its lifecycle, a ranker standing on them, the admitted
/// candidates and what lies outside the span, and, under the anchored gate,
/// the question's contract.
///
/// The answer and the doubt band read the same setup, so a judge is asked
/// about exactly what the answer will stand on.
pub(super) struct AskSetup<'a> {
    pub(super) admission: TemporalAdmission,
    pub(super) bounded: Cow<'a, KmpBundle>,
    pub(super) superseded_refs: BTreeSet<String>,
    pub(super) ranker: AnswerEvidenceRanker<'a>,
    pub(super) candidate_evidence: Vec<MemoryEvidence>,
    pub(super) outside_evidence: Vec<MemoryEvidence>,
    pub(super) contract: Option<QuestionContract>,
    /// The anchors the question excluded, and those it asked about.
    negated: BTreeSet<String>,
    asked_anchors: BTreeSet<String>,
}

impl<'a> AskSetup<'a> {
    /// `gated` reads every memory with the alias terms it spells and the
    /// question through its contract.
    pub(super) fn read(
        result: &'a GetContextResult,
        question: &str,
        temporal: &TemporalSelection,
        bridge: &'a LexicalBridge,
        cache: Option<&'a LexicalIndexCache>,
        gated: bool,
    ) -> ProtoMappingResult<Self> {
        let lexical_identity = LexicalIndexIdentity::read(result, temporal);
        let admission = TemporalAdmission::read(&result.bundle, temporal)?;
        let bounded = admission.bound(&result.bundle);
        let lifecycle = lifecycle_for(&bounded, &admission);
        let superseded_refs = lifecycle.superseded_refs().clone();
        let ranker = AnswerEvidenceRanker::from_bundle_at(&bounded, bridge, lifecycle)
            .with_lexical_cache(cache, lexical_identity);
        let ranker = if gated {
            ranker.with_identifier_aliases()
        } else {
            ranker
        };
        // What the selection admits is decided before the ranker weighs a
        // word, so the collection its statistics read is the selection's
        // own: a word common in the about and rare in the span earns what it
        // earns there.
        let (mut candidate_evidence, outside_evidence): (Vec<_>, Vec<_>) =
            answer_evidence_from_bundle(&result.bundle)
                .into_iter()
                .partition(|item| admission.admits(item));
        for evidence in &mut candidate_evidence {
            admission.bound_supports(evidence);
        }
        let contract = gated.then(|| QuestionContract::read(question, ranker.morphology()));
        let negated = contract
            .as_ref()
            .map(QuestionContract::negated_terms)
            .unwrap_or_default();
        let asked_anchors = contract
            .as_ref()
            .map(QuestionContract::unnegated_terms)
            .unwrap_or_default();
        Ok(Self {
            admission,
            bounded,
            superseded_refs,
            ranker,
            candidate_evidence,
            outside_evidence,
            contract,
            negated,
            asked_anchors,
        })
    }

    /// What the ranker reads: the question, or under the gate the question
    /// without what it excluded and with its anchors' alias terms.
    pub(super) fn asked<'q>(&'q self, question: &'q str) -> &'q str {
        self.contract
            .as_ref()
            .and_then(QuestionContract::asked)
            .unwrap_or(question)
    }

    /// Whether a memory names only anchors the question excluded
    /// (`excluding C7`), so it stays out of an unanchored core.
    pub(super) fn negated_only(&self, item: &MemoryEvidence) -> bool {
        !self.negated.is_empty()
            && self.ranker.memory_names_any(item, &self.negated)
            && !self.ranker.memory_names_any(item, &self.asked_anchors)
    }

    /// Whether the question excluded an identifier (`excluding C7`).
    pub(super) fn excludes_an_anchor(&self) -> bool {
        !self.negated.is_empty()
    }

    pub(super) fn policy_is_strict(policy: MemoryAnswerPolicy) -> bool {
        matches!(
            policy,
            MemoryAnswerPolicy::EvidenceOrUnknown | MemoryAnswerPolicy::ShowConflicts
        )
    }
}
