use std::collections::BTreeMap;

/// Okapi BM25 over the candidates of one question.
///
/// The ranker used to count how many distinct concepts a candidate shared
/// with the question, unweighted. Matching `valkey` scored exactly what
/// matching `store` scored, a candidate that said a word four times scored
/// what one that said it once scored, and a long entry beat a short one for
/// having more surface to hit. BM25 is the thirty-year-old answer to all
/// three: rare terms weigh more, repetition saturates, and length is
/// normalized away.
///
/// The collection is this question's own candidates, not a global corpus.
/// That is deliberate: inside an about whose every entry mentions deploys,
/// `deploy` genuinely carries no information, and an IDF measured here says
/// so. It also means the weights move as memory grows, which is why nothing
/// downstream stores them.
#[derive(Debug, Default)]
pub(super) struct LexicalField {
    documents: usize,
    document_frequency: BTreeMap<String, usize>,
    average_length: f64,
}

/// Standard Okapi parameters. `k1` sets how fast repetition saturates and `b`
/// how hard length is normalized; these are the values the literature settled
/// on, and nothing here is tuned against a benchmark that does not exist yet.
const K1: f64 = 1.2;
const B: f64 = 0.75;

/// Ordering runs on tenths of a BM25 point.
///
/// Fine enough that a clearly better text match wins on its own, coarse
/// enough that two candidates the question cannot separate fall through to
/// the typed relation and the clock instead of being decided by float noise.
const SCORE_SCALE: f64 = 10.0;

impl LexicalField {
    /// Builds the field statistics from every candidate's term counts.
    pub(super) fn build<'a>(documents: impl IntoIterator<Item = &'a TermCounts>) -> Self {
        let mut document_frequency = BTreeMap::<String, usize>::new();
        let mut total_length = 0usize;
        let mut count = 0usize;
        for document in documents {
            count += 1;
            total_length += document.length();
            for term in document.terms() {
                *document_frequency.entry(term.clone()).or_default() += 1;
            }
        }
        Self {
            documents: count,
            document_frequency,
            average_length: if count == 0 {
                0.0
            } else {
                total_length as f64 / count as f64
            },
        }
    }

    /// How much one concept tells us, by the smoothed probabilistic IDF that
    /// keeps every weight positive.
    pub(super) fn inverse_document_frequency(&self, term: &str) -> f64 {
        let documents = self.documents as f64;
        let frequency = self.document_frequency.get(term).copied().unwrap_or(0) as f64;
        (1.0 + (documents - frequency + 0.5) / (frequency + 0.5)).ln()
    }

    /// The BM25 score of one candidate against one question.
    pub(super) fn score(&self, question: &TermCounts, document: &TermCounts) -> f64 {
        if self.documents == 0 || self.average_length <= 0.0 {
            return 0.0;
        }
        let length_ratio = document.length() as f64 / self.average_length;
        question
            .terms()
            .map(|term| {
                let frequency = document.count(term) as f64;
                if frequency == 0.0 {
                    return 0.0;
                }
                let saturation =
                    frequency * (K1 + 1.0) / (frequency + K1 * (1.0 - B + B * length_ratio));
                self.inverse_document_frequency(term) * saturation
            })
            .sum()
    }

    /// What one occurrence is worth in a document of this length.
    ///
    /// Length normalization belongs to how well a candidate answers, not to
    /// whether it may answer at all. A floor fixed at the average length
    /// refuses a longer-than-average candidate that matches exactly the
    /// concept the floor is made of — which is a cliff again, wearing the
    /// units of the thing meant to remove one.
    pub(super) fn single_occurrence_factor(&self, document: &TermCounts) -> f64 {
        if self.documents == 0 || self.average_length <= 0.0 {
            return 1.0;
        }
        let length_ratio = document.length() as f64 / self.average_length;
        (K1 + 1.0) / (1.0 + K1 * (1.0 - B + B * length_ratio))
    }

    /// What a candidate must earn to be answering the question at all.
    ///
    /// The old floor counted words: one shared concept for a short question,
    /// two for a longer one. That is a rule about question length pretending
    /// to be a rule about relevance, and it is what discarded — unscored —
    /// the candidate that matched the single word actually identifying the
    /// subject.
    ///
    /// The floor is the median informativeness of the question's concepts, met
    /// at one occurrence in an average-length candidate. Matching one rare
    /// word clears it; matching only words every candidate uses does not.
    ///
    /// Two details keep it from becoming a new cliff. Concepts nothing in the
    /// collection carries are left out: no candidate can ever earn them, so
    /// letting them raise the bar would refuse everyone. And the middle is
    /// taken rather than the mean, because one very rare word in an otherwise
    /// ordinary question would drag a mean up until only that word mattered —
    /// an AND on a single term wearing a threshold's clothes.
    pub(super) fn eligibility_floor(&self, question: &TermCounts) -> f64 {
        if self.documents == 0 {
            return 0.0;
        }
        let mut weights = question
            .terms()
            .filter(|term| self.document_frequency.contains_key(term.as_str()))
            .map(|term| self.inverse_document_frequency(term))
            .collect::<Vec<_>>();
        if weights.is_empty() {
            return 0.0;
        }
        weights.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        let middle = weights.len() / 2;
        if weights.len() % 2 == 0 {
            (weights[middle - 1] + weights[middle]) / 2.0
        } else {
            weights[middle]
        }
    }
}

/// Quantizes a score for ordering, so comparison is exact and reproducible
/// rather than a float comparison that could panic on a value that cannot
/// occur but is not forbidden by the type.
pub(super) fn ranked_score(score: f64) -> i64 {
    if !score.is_finite() {
        return 0;
    }
    (score * SCORE_SCALE).round() as i64
}

/// How often each concept occurs in one text, and how long that text is.
///
/// Counting is what BM25 needs and what the ranker's term sets threw away.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct TermCounts {
    counts: BTreeMap<String, u32>,
    length: usize,
}

impl TermCounts {
    pub(super) fn insert(&mut self, term: String) {
        *self.counts.entry(term).or_default() += 1;
        self.length += 1;
    }

    pub(super) fn terms(&self) -> impl Iterator<Item = &String> {
        self.counts.keys()
    }

    pub(super) fn count(&self, term: &str) -> u32 {
        self.counts.get(term).copied().unwrap_or(0)
    }

    pub(super) fn length(&self) -> usize {
        self.length
    }
}

impl FromIterator<String> for TermCounts {
    fn from_iter<I: IntoIterator<Item = String>>(terms: I) -> Self {
        let mut counts = Self::default();
        for term in terms {
            counts.insert(term);
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(terms: &[&str]) -> TermCounts {
        terms.iter().map(|term| (*term).to_string()).collect()
    }

    #[test]
    fn a_rare_concept_weighs_more_than_a_common_one() {
        let documents = [
            counts(&["store", "valkey"]),
            counts(&["store", "sqlite"]),
            counts(&["store", "redb"]),
            counts(&["store", "backup"]),
        ];
        let field = LexicalField::build(documents.iter());

        assert!(
            field.inverse_document_frequency("valkey") > field.inverse_document_frequency("store")
        );
    }

    #[test]
    fn matching_the_rare_concept_outscores_matching_the_common_one() {
        let documents = [
            counts(&["store", "valkey"]),
            counts(&["store", "sqlite"]),
            counts(&["store", "redb"]),
            counts(&["store", "backup"]),
        ];
        let field = LexicalField::build(documents.iter());
        let question = counts(&["store", "valkey"]);

        assert!(field.score(&question, &documents[0]) > field.score(&question, &documents[1]));
    }

    #[test]
    fn repetition_saturates_instead_of_accumulating() {
        let once = counts(&["valkey", "store"]);
        let many = counts(&["valkey", "valkey", "valkey", "valkey", "store"]);
        let field = LexicalField::build([&once, &many]);
        let question = counts(&["valkey"]);

        let single = field.score(&question, &once);
        let repeated = field.score(&question, &many);
        assert!(repeated > single);
        assert!(
            repeated < single * 4.0,
            "four occurrences must not score four times one"
        );
    }

    #[test]
    fn length_does_not_buy_relevance() {
        let short = counts(&["valkey", "store"]);
        let padded = counts(&[
            "valkey", "store", "note", "meeting", "agenda", "calendar", "invite", "room",
        ]);
        let field = LexicalField::build([&short, &padded]);
        let question = counts(&["valkey"]);

        assert!(
            field.score(&question, &short) > field.score(&question, &padded),
            "the longer candidate must not win for having more surface"
        );
    }

    #[test]
    fn one_rare_match_clears_the_floor_that_one_common_match_does_not() {
        let documents = [
            counts(&["store", "valkey"]),
            counts(&["store", "sqlite"]),
            counts(&["store", "sqlite"]),
            counts(&["store", "backup"]),
            counts(&["store", "snapshot"]),
        ];
        let field = LexicalField::build(documents.iter());
        // Three concepts, so the rule this replaces demanded two shared words
        // and would have discarded the rare single match unscored.
        let question = counts(&["store", "valkey", "sqlite"]);
        let floor = field.eligibility_floor(&question);

        assert!(field.score(&question, &counts(&["valkey", "note"])) >= floor);
        assert!(field.score(&question, &counts(&["store", "note"])) < floor);
    }

    /// A word nobody wrote cannot be part of what everyone must earn.
    #[test]
    fn a_concept_absent_from_the_collection_does_not_raise_the_bar() {
        let documents = [counts(&["store", "valkey"]), counts(&["store", "sqlite"])];
        let field = LexicalField::build(documents.iter());

        let grounded = counts(&["store", "valkey"]);
        let with_absent_word = counts(&["store", "valkey", "kubernetes"]);

        assert_eq!(
            field.eligibility_floor(&grounded),
            field.eligibility_floor(&with_absent_word)
        );
    }

    /// The floor has to speak the same units as the score it is compared
    /// against, or length normalization turns into a membership rule.
    #[test]
    fn the_floor_follows_the_length_of_what_it_judges() {
        let short = counts(&["valkey", "store"]);
        let long = counts(&[
            "valkey", "store", "note", "meeting", "agenda", "calendar", "invite", "room",
        ]);
        let field = LexicalField::build([&short, &long]);
        let question = counts(&["valkey"]);
        let floor = field.eligibility_floor(&question);

        assert!(field.single_occurrence_factor(&long) < field.single_occurrence_factor(&short));
        assert!(
            field.score(&question, &long) < floor,
            "the fixture must exercise the case a fixed floor refuses"
        );
        assert!(field.score(&question, &long) >= floor * field.single_occurrence_factor(&long));
    }

    #[test]
    fn an_empty_collection_scores_nothing_and_refuses_no_one() {
        let field = LexicalField::build(std::iter::empty());

        assert_eq!(field.score(&counts(&["valkey"]), &counts(&["valkey"])), 0.0);
        assert_eq!(field.eligibility_floor(&counts(&[])), 0.0);
        assert_eq!(field.eligibility_floor(&counts(&["valkey"])), 0.0);
    }

    #[test]
    fn ordering_runs_on_tenths_and_survives_a_value_that_cannot_occur() {
        assert_eq!(ranked_score(1.24), 12);
        assert_eq!(ranked_score(1.26), 13);
        assert_eq!(ranked_score(f64::NAN), 0);
    }

    #[test]
    fn counting_preserves_frequency_and_length() {
        let bag = counts(&["valkey", "store", "valkey"]);

        assert_eq!(bag.count("valkey"), 2);
        assert_eq!(bag.count("store"), 1);
        assert_eq!(bag.count("absent"), 0);
        assert_eq!(bag.length(), 3);
        assert_eq!(TermCounts::default().length(), 0);
    }
}
