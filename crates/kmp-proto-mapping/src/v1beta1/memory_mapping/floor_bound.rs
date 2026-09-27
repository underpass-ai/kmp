use std::collections::{BTreeMap, BTreeSet};

use super::bridged_key::BridgedKey;
use super::indexed_field_stats::IndexedFieldStats;
use super::indexed_question::IndexedQuestion;
use super::lexical_bridge::LexicalBridge;
use super::lexical_field::LexicalField;
use super::lexical_row::LexicalRow;
use super::search_terms::informative_term_counts;
use super::term_counts::TermCounts;

/// How far below the floor a candidate's bound must fall before it is left
/// unread. The bound and the ranker's score are both sums of a handful of
/// products and drift by a few ulps at most; this margin is far wider than
/// that and far narrower than any bound that matters.
const FLOOR_MARGIN: f64 = 1e-9;

/// MaxScore against the eligibility floor (DESIGN L6, P14): which of the
/// candidates the postings reached an ask can leave unread, because no
/// reading of the question can let them clear the floor, nor rescue them.
///
/// KMP's ask returns every candidate that clears the floor, ranked, so the
/// threshold MaxScore prunes against is the floor itself, not a k-th score.
/// The bound is sound for the whole [`super::relevance_key::RelevanceKey`]
/// because nothing below the floor is ranked at all
/// ([`super::answer_candidate::AnswerCandidate::eligible`] refuses it first):
///
/// - `clears_floor` asks `Σ w·idf·sat(tf, L) ≥ floor · sat(1, L)` over the
///   direct field, with `sat(tf, L) = tf(k1+1) / (tf + k1·n(L))`. Since
///   `sat(tf, L) / sat(1, L) ≤ tf` for every `tf ≥ 1` and every length, a
///   candidate with `Σ idf·tf < floor` over the question's words cannot clear
///   it, whatever its length. The words the reader wrote weigh one.
/// - Content is part of direct, so the content score adds nothing to this.
/// - Both readings (plain, and with the alias terms the anchored gate reads)
///   are bounded, each with its own idf and floor; a candidate is left unread
///   only when both refuse it.
/// - Every form the ranker may read the question in (as asked, and under the
///   gate without its negated stretches or with its alias terms) is bounded:
///   the words of all of them count, and the lowest floor any of them sets is
///   the one compared.
/// - The lexical bridge: the floor carries the words the table supplies,
///   exactly as [`super::lexicon::Lexicon`] builds it, and a candidate that
///   carries a bridged word is always read.
/// - The store's associations: a candidate that carries one is always read
///   (it can be rescued through it), and the associations themselves are
///   counted over every candidate that carries a word of the question
///   ([`super::indexed_ask::IndexedAsk::seed_rows`]), never over the ones read.
///
/// Where no sound bound exists the ask is not pruned at all ([`Self::read`]
/// returns `None`): a question whose anchors the gate requires (it ranks
/// without the floor's focus and reads anchors, not the floor), and a form
/// of the question with no informative word (the ranker then returns every
/// candidate). Search expansions (field X), time-scoped asks, several abouts
/// and dimensions are never held by the index, so they read the about.
///
/// MaxScore's partition makes the common case cheap: the question's words are
/// sorted by the most any candidate can earn from them (`idf · max tf`), and
/// those whose sum stays below the floor are non-essential. A candidate that
/// carries only non-essential words is refused without summing.
#[derive(Debug, Clone)]
pub struct FloorBound {
    readings: [ReadingBound; 2],
    /// Associations and bridged words: a candidate carrying one is read.
    kept: BTreeSet<String>,
}

/// One reading's floor and idf of every word of the question.
#[derive(Debug, Clone, Default)]
struct ReadingBound {
    floor: f64,
    idf: BTreeMap<String, f64>,
    essential: BTreeSet<String>,
}

impl FloorBound {
    /// The bound for `question` against the whole about's statistics under
    /// both readings, or `None` where no sound bound exists (above). `kept`
    /// are the associations and bridged words the postings were read for;
    /// `rows` every candidate reached, for the partition.
    pub fn read<'r>(
        question: &IndexedQuestion,
        plain: &IndexedFieldStats,
        aliased: &IndexedFieldStats,
        bridge: &LexicalBridge,
        vocabulary: Option<&[String]>,
        kept: BTreeSet<String>,
        rows: impl Iterator<Item = &'r LexicalRow> + Clone,
    ) -> Option<Self> {
        if question.requires_anchors() {
            return None;
        }
        let morphology = question.morphology();
        let mut asked = Vec::new();
        for form in question.forms() {
            let counts = informative_term_counts(form, morphology);
            if counts.terms().next().is_none() {
                return None;
            }
            // What the floor is made of: the question's words and, where
            // the table supplied one it lacks, the candidate's word.
            let bridged = match vocabulary {
                Some(vocabulary) => BridgedKey::read_words(form, morphology, vocabulary, bridge),
                None => Vec::new(),
            };
            let mut asked_for = counts.clone();
            let mut supplied = BTreeSet::new();
            for pair in &bridged {
                if counts.count(&pair.candidate_key) == 0
                    && supplied.insert(pair.candidate_key.clone())
                {
                    asked_for.insert(pair.candidate_key.clone());
                }
            }
            asked.push((counts, asked_for));
        }
        let words = asked
            .iter()
            .flat_map(|(counts, _)| counts.terms().cloned())
            .collect::<BTreeSet<_>>();
        let readings = [false, true].map(|reading| {
            let stats = if reading { aliased } else { plain };
            ReadingBound::read(stats, &asked, &words, reading, rows.clone())
        });
        Some(Self { readings, kept })
    }

    /// Whether `row` can be left unread: it carries no association or
    /// bridged word, and under neither reading can it clear the floor.
    pub fn prunes(&self, row: &LexicalRow) -> bool {
        if row
            .terms()
            .iter()
            .any(|term| term.is_held() && self.kept.contains(&term.term))
        {
            return false;
        }
        self.readings
            .iter()
            .zip([false, true])
            .all(|(bound, aliased)| bound.refuses(row, aliased))
    }
}

impl ReadingBound {
    fn read<'r>(
        stats: &IndexedFieldStats,
        asked: &[(TermCounts, TermCounts)],
        words: &BTreeSet<String>,
        aliased: bool,
        rows: impl Iterator<Item = &'r LexicalRow>,
    ) -> Self {
        let field =
            LexicalField::from_stats(stats.documents, stats.direct_length, &stats.direct_df);
        let floor = asked
            .iter()
            .map(|(_, asked_for)| field.eligibility_floor(asked_for))
            .fold(f64::INFINITY, f64::min);
        let idf = words
            .iter()
            .map(|word| (word.clone(), field.inverse_document_frequency(word)))
            .collect::<BTreeMap<_, _>>();
        // MaxScore's partition: the most each word can add, smallest first;
        // the words whose running sum stays under the floor are not enough
        // on their own.
        let mut most = BTreeMap::<&str, i64>::new();
        for row in rows {
            for term in row.terms() {
                if idf.contains_key(&term.term) {
                    let count = term.direct(aliased);
                    let entry = most.entry(term.term.as_str()).or_default();
                    *entry = (*entry).max(count);
                }
            }
        }
        let mut earned = most
            .iter()
            .map(|(word, count)| (idf[*word] * *count as f64, *word))
            .collect::<Vec<_>>();
        earned.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(right.1)));
        let mut essential = BTreeSet::new();
        let mut running = 0.0;
        for (most, word) in earned {
            running += most;
            if running >= Self::bar(floor) {
                essential.insert(word.to_string());
            }
        }
        Self {
            floor,
            idf,
            essential,
        }
    }

    fn bar(floor: f64) -> f64 {
        floor * (1.0 - FLOOR_MARGIN)
    }

    /// Whether no length lets `row` clear this reading's floor.
    fn refuses(&self, row: &LexicalRow, aliased: bool) -> bool {
        if !(self.floor.is_finite() && self.floor > 0.0) {
            return false;
        }
        let carried = row
            .terms()
            .iter()
            .filter(|term| term.direct(aliased) > 0 && self.idf.contains_key(&term.term));
        if carried
            .clone()
            .all(|term| !self.essential.contains(&term.term))
        {
            return true;
        }
        let bound = carried
            .map(|term| self.idf[&term.term] * term.direct(aliased) as f64)
            .sum::<f64>();
        bound < Self::bar(self.floor)
    }
}
