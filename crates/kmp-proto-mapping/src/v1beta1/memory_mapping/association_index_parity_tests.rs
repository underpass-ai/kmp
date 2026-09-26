//! The question-restricted PMI against the whole-store oracle.
//!
//! `AssociationIndex::for_question` must give every question term exactly the
//! neighbours `AssociationIndex::build` gives it: the same words, the same
//! order and the same float bits. The corpora are random but deterministic
//! (a fixed SplitMix64 stream per case), and they are shaped to sit on the
//! edges that matter: the twelve-document floor, pairs seen exactly three
//! times, and terms whose PMI bound ln(N/df) lands on either side of the 0.5
//! bar.
use super::association_index::AssociationIndex;
use super::lexical_field::LexicalField;
use super::term_counts::TermCounts;

/// SplitMix64: enough randomness for corpora, and the same stream every run.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

fn word(index: usize) -> String {
    format!("w{index:03}")
}

/// A corpus whose term frequencies follow a skewed law, so both common words
/// (bound below the bar) and rare ones (bound above it) are present.
fn corpus(seed: u64, documents: usize, vocabulary: usize, length: usize) -> Vec<TermCounts> {
    let mut stream = Stream(seed);
    (0..documents)
        .map(|_| {
            let words = 1 + stream.below(length);
            (0..words)
                .map(|_| {
                    // The root of a uniform draw favours high roots, so
                    // counting down from the top makes w000 common and the
                    // tail rare.
                    let draw = stream.below(vocabulary * vocabulary);
                    word(vocabulary - 1 - (draw as f64).sqrt() as usize)
                })
                .collect()
        })
        .collect()
}

fn question_of(words: &[String]) -> TermCounts {
    words.iter().cloned().collect()
}

/// Every term the corpus names, plus one it never does.
fn every_term(documents: &[TermCounts]) -> Vec<String> {
    let mut terms = documents
        .iter()
        .flat_map(|document| document.terms().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    terms.push("absent".to_string());
    terms
}

fn assert_same_neighbours(documents: &[TermCounts], question: &TermCounts, case: &str) {
    let oracle = AssociationIndex::build(documents.iter());
    let field = LexicalField::build(documents.iter());
    let restricted = AssociationIndex::for_question(question, &field, documents.iter());
    for term in question.terms() {
        let expected = oracle.neighbours_of(term).unwrap_or_default();
        let actual = restricted.neighbours_of(term).unwrap_or_default();
        assert_eq!(
            expected
                .iter()
                .map(|(word, weight)| (word.as_str(), weight.to_bits()))
                .collect::<Vec<_>>(),
            actual
                .iter()
                .map(|(word, weight)| (word.as_str(), weight.to_bits()))
                .collect::<Vec<_>>(),
            "{case}: neighbours of {term}"
        );
    }
    // Nothing outside the question is computed, and expansion is identical.
    assert!(
        restricted.terms().all(|term| question.count(term) > 0),
        "{case}: restricted index computed a term the question did not ask"
    );
    let expected = oracle.expand(question);
    let actual = restricted.expand(question);
    assert_eq!(
        expected
            .iter()
            .map(|(word, weight)| (word.as_str(), weight.to_bits()))
            .collect::<Vec<_>>(),
        actual
            .iter()
            .map(|(word, weight)| (word.as_str(), weight.to_bits()))
            .collect::<Vec<_>>(),
        "{case}: expansion"
    );
}

#[test]
fn restricted_neighbours_match_the_whole_store_build_on_random_corpora() {
    let mut with_neighbours = 0usize;
    for seed in 0..48u64 {
        let documents = 12 + (seed as usize * 7) % 90;
        let vocabulary = 8 + (seed as usize * 5) % 40;
        let length = 2 + (seed as usize) % 14;
        let docs = corpus(seed, documents, vocabulary, length);
        let terms = every_term(&docs);
        let oracle = AssociationIndex::build(docs.iter());
        with_neighbours += oracle.terms().count();
        // Each term alone, so every term the oracle knows is checked...
        for term in &terms {
            assert_same_neighbours(
                &docs,
                &question_of(std::slice::from_ref(term)),
                &format!("seed {seed} term {term}"),
            );
        }
        // ...and whole questions, whose terms share one pass.
        let mut stream = Stream(seed ^ 0xA5A5);
        for round in 0..6 {
            let size = 1 + stream.below(5);
            let words = (0..size)
                .map(|_| terms[stream.below(terms.len())].clone())
                .collect::<Vec<_>>();
            assert_same_neighbours(
                &docs,
                &question_of(&words),
                &format!("seed {seed} question {round}"),
            );
        }
    }
    assert!(
        with_neighbours > 100,
        "the corpora must produce associations to compare: {with_neighbours}"
    );
}

/// Below twelve documents both decline; at twelve both measure.
#[test]
fn the_twelve_document_floor_is_the_same_for_both() {
    let pair = || question_of(&["cache".to_string(), "valkey".to_string()]);
    let document = |terms: &[&str]| terms.iter().map(|t| (*t).to_string()).collect();
    for size in 10..=14 {
        let docs = (0..size)
            .map(|index| {
                if index < 4 {
                    document(&["cache", "valkey"])
                } else {
                    document(&["meeting", "agenda"])
                }
            })
            .collect::<Vec<TermCounts>>();
        let field = LexicalField::build(docs.iter());
        let restricted = AssociationIndex::for_question(&pair(), &field, docs.iter());
        assert_eq!(
            restricted.neighbours_of("cache").is_some(),
            size >= 12,
            "size {size}"
        );
        assert_same_neighbours(&docs, &pair(), &format!("floor {size}"));
    }
}

/// Terms whose bound ln(N/df) sits just below, on and just above 0.5, and
/// pairs seen exactly two, three and four times.
///
/// With x everywhere t is (c = df_t = df_x), PMI(t,x) = ln(N/df_t): the bound
/// is reached. N/df ranges over ratios around e^0.5 ≈ 1.6487, so the bar is
/// crossed within this sweep in both directions.
#[test]
fn bound_and_pair_count_edges_match_the_whole_store_build() {
    let (mut kept, mut refused) = (0usize, 0usize);
    for total in 12..=40usize {
        for frequency in 2..=total {
            let docs = (0..total)
                .map(|index| {
                    let mut terms = vec![format!("filler{}", index % 5)];
                    if index < frequency {
                        terms.push("t".to_string());
                        terms.push("x".to_string());
                    }
                    // A partner seen with t in only the first three (or
                    // fewer) of its documents.
                    if index < frequency.min(3) {
                        terms.push("y".to_string());
                    }
                    terms.into_iter().collect()
                })
                .collect::<Vec<TermCounts>>();
            let bound = (total as f64 / frequency as f64).ln();
            let question = question_of(&["t".to_string(), "y".to_string()]);
            assert_same_neighbours(
                &docs,
                &question,
                &format!("N {total} df {frequency} bound {bound}"),
            );
            if frequency >= 3 && (bound - 0.5).abs() < 0.05 {
                let oracle = AssociationIndex::build(docs.iter());
                match oracle.neighbours_of("t") {
                    Some(found) if found.iter().any(|(word, _)| word == "x") => kept += 1,
                    _ => refused += 1,
                }
            }
        }
    }
    assert!(
        kept > 0 && refused > 0,
        "the sweep must cross the bar both ways near 0.5: kept {kept}, refused {refused}"
    );
}

/// The skip is exact: a term the bound excludes has no neighbours in the
/// whole-store build either, over every term of the random corpora.
#[test]
fn a_term_the_bound_skips_has_no_neighbour_in_the_whole_store_build() {
    let mut skipped = 0usize;
    for seed in 0..24u64 {
        let docs = corpus(seed, 40 + seed as usize, 6 + seed as usize % 6, 6);
        let oracle = AssociationIndex::build(docs.iter());
        let field = LexicalField::build(docs.iter());
        for term in every_term(&docs) {
            let frequency = field.document_frequency(&term);
            if frequency == 0 {
                continue;
            }
            if (docs.len() as f64 / frequency as f64).ln() < 0.5 - 1e-9 {
                skipped += 1;
                assert!(oracle.neighbours_of(&term).is_none(), "seed {seed} {term}");
            }
        }
    }
    assert!(skipped > 0, "the corpora must exercise the skip");
}
