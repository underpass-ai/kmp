use super::answer_candidate_terms::AnswerCandidateTerms;
use super::association_index::AssociationIndex;
use super::bridged_key::BridgedKey;
use super::bridged_term::BridgedTerm;
use super::lexical_bridge::LexicalBridge;
use super::lexical_collection::LexicalCollection;
use super::lexical_field::ranked_score;
use super::morphology::Morphology;
use super::search_terms::informative_term_counts;
use super::term_counts::TermCounts;
use kmp_proto::v1beta1::MemoryEvidence;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Everything BM25 needs about one question and the candidates it is being
/// asked against.
pub(super) struct Lexicon {
    question: TermCounts,
    /// The question as asked, plus what this memory says goes with it, plus
    /// what the table says it means in the candidates' language. The words
    /// the reader wrote weigh one; the store's own associations weigh a
    /// fraction; a bridged word weighs what the table thinks it is worth.
    asked: BTreeMap<String, f64>,
    /// The question plus the store's own associations only, so a candidate
    /// the table bridged is not mistaken for one the store's vocabulary
    /// reached.
    associated: BTreeMap<String, f64>,
    collection: Arc<LexicalCollection>,
    floor: f64,
    /// The word pairs the table vouched for between this question and these
    /// candidates, so a hit can say which ones carried it.
    bridged: Vec<BridgedKey>,
}

impl Lexicon {
    pub(super) fn build(
        question: &str,
        morphology: &Morphology,
        prepared: &[(MemoryEvidence, AnswerCandidateTerms)],
        bridge: &LexicalBridge,
        collection: Arc<LexicalCollection>,
        vocabulary: Option<&[String]>,
        association_documents: Option<&[TermCounts]>,
    ) -> Self {
        let question_counts = informative_term_counts(question, morphology);
        // The table bridges the question to the words of every candidate:
        // those given, or the whole about's when the lexical index gave only
        // the candidates its postings reached (DESIGN L6, P13).
        let bridged = match vocabulary {
            Some(vocabulary) => BridgedKey::read_words(question, morphology, vocabulary, bridge),
            None => BridgedKey::read(
                question,
                morphology,
                prepared.iter().map(|(item, _)| item.text.as_str()),
                bridge,
            ),
        };
        // The bar stays what the reader asked for — in the store's words
        // where the table had to supply them. Expansion may help a candidate
        // clear it; it may not lower it.
        let mut asked_for = question_counts.clone();
        let mut supplied = BTreeSet::new();
        for pair in &bridged {
            if question_counts.count(&pair.candidate_key) == 0
                && supplied.insert(pair.candidate_key.as_str())
            {
                asked_for.insert(pair.candidate_key.clone());
            }
        }
        let floor = collection.direct.eligibility_floor(&asked_for);
        // Associations are counted over every candidate that carries a word
        // of the question: those given, or, when the lexical index left the
        // ones below the floor unread (P14), the ones its postings reached.
        let associations = match association_documents {
            Some(documents) => {
                AssociationIndex::for_question(&question_counts, &collection.direct, documents)
            }
            None => AssociationIndex::for_question(
                &question_counts,
                &collection.direct,
                prepared.iter().map(|(_, terms)| &terms.direct_counts),
            ),
        };
        let associated = associations.expand(&question_counts);
        let mut asked = associated.clone();
        for pair in &bridged {
            let weight = asked.entry(pair.candidate_key.clone()).or_insert(0.0);
            if pair.term.similarity > *weight {
                *weight = pair.term.similarity;
            }
        }
        Self {
            question: question_counts,
            asked,
            associated,
            collection,
            floor,
            bridged,
        }
    }

    /// Every term the question weighs: its own words, their associations in
    /// this memory and the words the table bridged them to.
    pub(super) fn weighted_terms(&self) -> impl Iterator<Item = &String> {
        self.asked.keys()
    }

    /// Whether a candidate says enough about the question to be answering it.
    ///
    /// Measured on the raw score, not the quantized one, so a candidate is
    /// never refused by a rounding boundary.
    pub(super) fn clears_floor(&self, terms: &AnswerCandidateTerms) -> bool {
        let score = self
            .collection
            .direct
            .score_weighted(&self.asked, &terms.direct_counts);
        // The bar is read in the candidate's own length, so a long entry that
        // says the one thing the question asked about is ranked low rather
        // than refused.
        let floor = self.floor
            * self
                .collection
                .direct
                .single_occurrence_factor(&terms.direct_counts);
        // Sharing nothing at all is its own answer, and no floor derived from
        // an empty overlap should be able to admit it.
        score > 0.0 && score >= floor
    }

    /// Whether the store's own vocabulary carries the question to a candidate
    /// its words did not reach.
    ///
    /// The test is only that an expansion is what made the difference. The
    /// selectivity already happened where it can be measured and tuned: the
    /// index refuses a memory too small to have a pattern, a pair seen too few
    /// times to be one, an association no stronger than chance, and more than
    /// three neighbours for any word. Charging a second floor on top would
    /// charge the same care twice, and a candidate reached this way carries
    /// less than a whole concept by construction — which is exactly why it
    /// arrives marked and stays out of the answer.
    pub(super) fn is_associated(&self, terms: &AnswerCandidateTerms) -> bool {
        let expanded = self
            .collection
            .direct
            .score_weighted(&self.associated, &terms.direct_counts);
        if expanded <= 0.0 {
            return false;
        }
        let asked_for = self
            .question
            .terms()
            .map(|term| (term.clone(), 1.0))
            .collect::<BTreeMap<_, _>>();
        expanded
            > self
                .collection
                .direct
                .score_weighted(&asked_for, &terms.direct_counts)
    }

    /// The word pairs the table bridged onto this candidate, in the order the
    /// question asked them.
    pub(super) fn bridged_pairs(&self, terms: &AnswerCandidateTerms) -> Vec<&BridgedTerm> {
        BridgedKey::landing_on(&self.bridged, terms)
    }

    /// How many of the focus concepts this candidate answers, in its own words
    /// or through the table.
    pub(super) fn focus_matches(
        &self,
        focus_terms: &BTreeSet<String>,
        terms: &AnswerCandidateTerms,
    ) -> usize {
        BridgedKey::focus_matches(&self.bridged, focus_terms, terms)
    }

    /// How many of the focus concepts this candidate states in its own
    /// words, or through the table onto them: its content alone.
    pub(super) fn content_focus_matches(
        &self,
        focus_terms: &BTreeSet<String>,
        terms: &AnswerCandidateTerms,
    ) -> usize {
        BridgedKey::content_matches(&self.bridged, focus_terms, terms)
    }

    pub(super) fn content_score(&self, terms: &AnswerCandidateTerms) -> i64 {
        self.content_score_of(&terms.content_counts)
    }

    pub(super) fn direct_score(&self, terms: &AnswerCandidateTerms) -> i64 {
        self.direct_score_of(&terms.direct_counts)
    }

    /// [`Self::content_score`] over counts read elsewhere (the lexical
    /// index's rows, P14).
    pub(super) fn content_score_of(&self, content: &TermCounts) -> i64 {
        ranked_score(self.collection.content.score_weighted(&self.asked, content))
    }

    /// [`Self::direct_score`] over counts read elsewhere.
    pub(super) fn direct_score_of(&self, direct: &TermCounts) -> i64 {
        ranked_score(self.collection.direct.score_weighted(&self.asked, direct))
    }
}
