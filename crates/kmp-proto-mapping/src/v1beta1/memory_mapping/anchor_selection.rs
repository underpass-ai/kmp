use super::question_anchor::QuestionAnchor;
use super::question_contract::QuestionContract;
use super::question_contract_vocabulary::QuestionContractVocabulary;

/// What the admitted candidates say about a question's required anchors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AnchorSelection {
    /// No anchor is required once hubs are read as words: the ordinary
    /// ⌈2/3⌉ rule decides.
    Unanchored,
    /// A required anchor no admitted candidate carries, as the question
    /// wrote each one. Not "not stored": it may be written another way, or
    /// live in an about or a span this read did not select.
    Absent(Vec<String>),
    /// The rarest required anchor, which every cited memory must name, and
    /// the others, which the cited memories together must name.
    Anchored {
        principal: QuestionAnchor,
        others: Vec<QuestionAnchor>,
    },
}

impl AnchorSelection {
    /// Reads the required anchors against their document frequency among the
    /// `documents` admitted candidates.
    ///
    /// An anchor most candidates carry is a hub, not a subject, and is read
    /// as a word. The principal anchor is the rarest of the rest (the
    /// specificity `1/|P_i|` of HippoRAG, 2405.14831), the first named on a
    /// tie.
    pub(super) fn read(
        contract: &QuestionContract,
        document_frequency: impl Fn(&str) -> usize,
        documents: usize,
    ) -> Self {
        let vocabulary = QuestionContractVocabulary::shipped();
        let required = contract
            .anchors()
            .iter()
            .filter(|anchor| anchor.is_required())
            .map(|anchor| (anchor, document_frequency(&anchor.term)))
            .filter(|(_, frequency)| !vocabulary.is_hub(*frequency, documents))
            .collect::<Vec<_>>();
        let absent = required
            .iter()
            .filter(|(_, frequency)| *frequency == 0)
            .map(|(anchor, _)| anchor.written.clone())
            .collect::<Vec<_>>();
        if !absent.is_empty() {
            return Self::Absent(absent);
        }
        let Some(principal) = required
            .iter()
            .enumerate()
            .min_by_key(|(position, (_, frequency))| (*frequency, *position))
            .map(|(position, _)| position)
        else {
            return Self::Unanchored;
        };
        let others = required
            .iter()
            .enumerate()
            .filter(|(position, _)| *position != principal)
            .map(|(_, (anchor, _))| (*anchor).clone())
            .collect();
        Self::Anchored {
            principal: required[principal].0.clone(),
            others,
        }
    }
}
