use std::collections::BTreeSet;
use std::sync::Arc;

use super::bridged_term::BridgedTerm;
use super::lexical_bridge_table::LexicalBridgeTable;

/// A table of whole words and the direction each one points in, so a question
/// can reach a memory that says the same thing in another language.
///
/// Everything above this in the ranker needs a word the question and the
/// memory share. BM25 weighs a shared word, morphology folds it onto its
/// other shapes, the association index learns which words this store uses
/// together, and the relation walk sets out from a candidate the question
/// already hit. `almacén` and `store` share nothing, and no store that uses
/// only one of them can ever learn the other. That knowledge has to come from
/// outside, and this is the shape it comes in: one vector per word, built
/// offline from a static embedding model, quantized to signed bytes, keyed by
/// the folded form the ranker already searches with.
///
/// Three properties make it fit the kernel where a model would not.
///
/// It is a lookup, not an inference. No tokenizer, no network, no threads:
/// the runtime reads bytes and multiplies integers.
///
/// It is bit-reproducible. Similarity is an integer dot product over integer
/// norms; the only floating-point operations are one exact conversion, one
/// square root and one division, each correctly rounded by IEEE 754, so the
/// same table gives the same number on every platform.
///
/// And it explains itself per pair. A candidate reached this way carries the
/// word pairs that bridged it — `valvula≈valve 0.51` — rather than a
/// sentence-level score nobody can audit. That is what a sentence embedding
/// cannot offer and what the kernel's contract asks for.
///
/// What it cannot do is worth saying plainly. A single vector per word has no
/// sense to pick, so `slipped` does not reach `postponed`: the paraphrase a
/// writer uses inside one language is not what the table knows, and a memory
/// that says it in the same language still needs to share a word. The table
/// bridges languages; it does not read minds.
///
/// Absent by default. A store with no table beside it behaves exactly as it
/// did, the same way a memory whose language cannot be read stems nothing.
#[derive(Clone, Debug, Default)]
pub struct LexicalBridge {
    table: Option<Arc<LexicalBridgeTable>>,
}

/// Below this two words are related, not the same word in two languages.
///
/// Measured on the MUSE Spanish–English test dictionary, 1,499 pairs, with
/// the shipped teacher at 64 dimensions and signed bytes: 87% of true
/// translation pairs sit at or above 0.45 and 0.1% of random pairs do. At
/// 0.40 the true rate is 90% and the random rate triples. The bar sits where
/// a false bridge is rarer than a missed one, because a missed bridge costs
/// what the kernel already cost before the table existed, and a false one
/// costs a citation.
pub(super) const MINIMUM_SIMILARITY: f64 = 0.45;

/// How many candidate words one question word may bridge to. A common word
/// must not widen a question across the whole candidate vocabulary.
const MAX_BRIDGES_PER_WORD: usize = 3;

impl LexicalBridge {
    /// No table at all. What every store gets until one is installed beside
    /// it, and what a malformed one degrades to.
    pub const fn none() -> Self {
        Self { table: None }
    }

    /// Reads a table in its own format, refusing anything that does not
    /// account for every byte.
    ///
    /// The format is deliberately plain: a fixed header, the words as one
    /// sorted blob with offsets, then the vectors. A reader that can verify
    /// it in a screenful of code is the point — the artifact is ours and so
    /// is the code that reads it.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        Self::from_owned_bytes(bytes.to_vec())
    }

    /// Reads and takes ownership of a table without copying its word and
    /// vector sections. File adapters should prefer this entry point after
    /// `std::fs::read`; callers with borrowed fixtures can use [`Self::from_bytes`].
    pub fn from_owned_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        Ok(Self {
            table: Some(Arc::new(LexicalBridgeTable::parse(bytes)?)),
        })
    }

    /// Whether there is nothing to bridge with. The behaviour it gates is
    /// visible in what gets bridged, so the kernel reads this only to skip
    /// the work.
    pub fn is_silent(&self) -> bool {
        self.is_empty()
    }

    pub fn len(&self) -> usize {
        self.table.as_deref().map_or(0, LexicalBridgeTable::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The model whose opinion the vectors carry.
    pub fn provenance(&self) -> &str {
        self.table
            .as_deref()
            .map_or("", LexicalBridgeTable::provenance)
    }

    /// The pairs the table vouches for between a question and a vocabulary.
    ///
    /// A pair is only offered when both words are in the table, they are not
    /// the same word — an exact match is BM25's to weigh — and the table
    /// puts them at or above the bar. Each question word brings its few best
    /// neighbours, strongest first and alphabetical on a tie, so the result
    /// is the same for the same inputs whatever order the vocabulary arrived
    /// in.
    pub(super) fn bridge(&self, question: &[String], vocabulary: &[String]) -> Vec<BridgedTerm> {
        if self.is_silent() {
            return Vec::new();
        }
        let vocabulary = vocabulary
            .iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|word| self.index_of(word).map(|index| (word, index)))
            .collect::<Vec<_>>();

        let mut bridged = Vec::new();
        let mut asked = BTreeSet::new();
        for word in question {
            if !asked.insert(word) {
                continue;
            }
            let Some(index) = self.index_of(word) else {
                continue;
            };
            let mut found = vocabulary
                .iter()
                .filter(|(candidate, _)| *candidate != word)
                .map(|(candidate, candidate_index)| {
                    (self.similarity_between(index, *candidate_index), *candidate)
                })
                .filter(|(similarity, _)| *similarity >= MINIMUM_SIMILARITY)
                .collect::<Vec<_>>();
            found.sort_by(|left, right| {
                right
                    .0
                    .partial_cmp(&left.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.1.cmp(right.1))
            });
            found.truncate(MAX_BRIDGES_PER_WORD);
            bridged.extend(
                found
                    .into_iter()
                    .map(|(similarity, candidate)| BridgedTerm {
                        question: word.clone(),
                        candidate: candidate.clone(),
                        similarity,
                    }),
            );
        }
        bridged
    }

    /// The cosine the table gives two words, or none when either is unknown.
    /// Exposed for tests and diagnostics; retrieval goes through `bridge`.
    pub fn similarity(&self, left: &str, right: &str) -> Option<f64> {
        let (left, right) = (self.index_of(left)?, self.index_of(right)?);
        Some(self.similarity_between(left, right))
    }

    fn index_of(&self, word: &str) -> Option<usize> {
        self.table.as_deref()?.index_of(word)
    }

    /// Cosine over integers. The dot product and both norms are exact
    /// integers well inside what an `f64` represents, so the conversion, the
    /// square root and the division are each correctly rounded and the
    /// result cannot differ between machines.
    fn similarity_between(&self, left: usize, right: usize) -> f64 {
        self.table
            .as_deref()
            .map_or(0.0, |table| table.similarity_between(left, right))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Encodes a table the way the builder does, from unit directions given
    /// as signed bytes. Words may arrive in any order; the encoding sorts.
    pub(crate) fn table(provenance: &str, entries: &[(&str, &[i8])]) -> Vec<u8> {
        let mut entries = entries.to_vec();
        entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        let dims = entries.first().map(|(_, vector)| vector.len()).unwrap_or(1);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(super::super::lexical_bridge_table::MAGIC);
        bytes.extend_from_slice(&super::super::lexical_bridge_table::VERSION.to_le_bytes());
        bytes.extend_from_slice(&(dims as u16).to_le_bytes());
        bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(provenance.len() as u16).to_le_bytes());
        bytes.extend_from_slice(provenance.as_bytes());
        let mut offset = 0u32;
        bytes.extend_from_slice(&offset.to_le_bytes());
        for (word, _) in &entries {
            offset += word.len() as u32;
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        for (word, _) in &entries {
            bytes.extend_from_slice(word.as_bytes());
        }
        for (_, vector) in &entries {
            bytes.extend(vector.iter().map(|value| *value as u8));
        }
        bytes
    }

    /// A toy space where `valve`/`valvula` and `night`/`noche` point the
    /// same way, `shift` and `turno` lean together, and the canteen points
    /// somewhere else entirely.
    pub(crate) fn spanish_english_toy() -> LexicalBridge {
        LexicalBridge::from_bytes(&table(
            "toy",
            &[
                ("valve", &[127, 0, 0, 0]),
                ("valvula", &[120, 40, 0, 0]),
                ("night", &[0, 127, 0, 0]),
                ("noche", &[10, 125, 0, 0]),
                ("shift", &[0, 0, 127, 0]),
                ("turno", &[0, 0, 120, 40]),
                ("canteen", &[0, 0, 0, 127]),
                ("meeting", &[-90, 0, 0, 90]),
            ],
        ))
        .expect("the toy table parses")
    }

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|word| (*word).to_string()).collect()
    }

    fn integer_cosine_oracle(left: &[i8], right: &[i8]) -> f64 {
        let dot = left.iter().zip(right).fold(0i64, |sum, (left, right)| {
            sum + i64::from(*left) * i64::from(*right)
        });
        let norm = |vector: &[i8]| {
            vector.iter().fold(0u64, |sum, value| {
                let magnitude = u64::from(value.unsigned_abs());
                sum + magnitude * magnitude
            })
        };
        let product = norm(left) * norm(right);
        if product == 0 {
            0.0
        } else {
            dot as f64 / (product as f64).sqrt()
        }
    }

    #[test]
    fn a_table_reads_back_its_words_and_their_provenance() {
        let bridge = spanish_english_toy();

        assert!(!bridge.is_silent());
        assert_eq!(bridge.len(), 8);
        assert_eq!(bridge.provenance(), "toy");
        assert_eq!(bridge.similarity("valve", "valve"), Some(1.0));
        assert!(
            bridge
                .similarity("valve", "valvula")
                .expect("the fixture is valid")
                > 0.9
        );
        assert!(
            bridge
                .similarity("valve", "canteen")
                .expect("the fixture is valid")
                < 0.01
        );
        assert_eq!(bridge.similarity("valve", "absent"), None);
    }

    #[test]
    fn similarity_is_the_same_number_however_it_is_asked() {
        let bridge = spanish_english_toy();

        assert_eq!(
            bridge.similarity("shift", "turno"),
            bridge.similarity("turno", "shift")
        );
        // Integer arithmetic underneath: 127·120 / sqrt(127² · (120² + 40²)).
        let expected = 127.0 * 120.0 / ((127.0f64 * 127.0) * (120.0 * 120.0 + 40.0 * 40.0)).sqrt();
        assert_eq!(bridge.similarity("shift", "turno"), Some(expected));
    }

    #[test]
    fn a_question_word_bridges_to_its_best_few_neighbours_above_the_bar() {
        let bridge = spanish_english_toy();

        let bridged = bridge.bridge(
            &words(&["valvula", "noche", "turno", "unknown"]),
            &words(&["valve", "night", "shift", "canteen", "meeting"]),
        );

        let pairs = bridged
            .iter()
            .map(|term| (term.question.as_str(), term.candidate.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            pairs,
            vec![("valvula", "valve"), ("noche", "night"), ("turno", "shift")]
        );
        assert!(
            bridged
                .iter()
                .all(|term| term.similarity >= MINIMUM_SIMILARITY)
        );
    }

    #[test]
    fn the_same_word_on_both_sides_is_not_a_bridge() {
        let bridge = spanish_english_toy();

        let bridged = bridge.bridge(&words(&["valve"]), &words(&["valve", "valvula"]));

        assert_eq!(bridged.len(), 1);
        assert_eq!(bridged[0].candidate, "valvula");
    }

    #[test]
    fn a_word_repeated_in_the_question_is_bridged_once() {
        let bridge = spanish_english_toy();

        let bridged = bridge.bridge(&words(&["noche", "noche"]), &words(&["night"]));

        assert_eq!(bridged.len(), 1);
    }

    #[test]
    fn one_word_cannot_bridge_to_more_than_a_few() {
        let bytes = table(
            "toy",
            &[
                ("hub", &[127, 0]),
                ("one", &[127, 1]),
                ("two", &[127, 2]),
                ("three", &[127, 3]),
                ("four", &[127, 4]),
                ("five", &[127, 5]),
            ],
        );
        let bridge = LexicalBridge::from_bytes(&bytes).expect("the fixture is valid");

        let bridged = bridge.bridge(
            &words(&["hub"]),
            &words(&["one", "two", "three", "four", "five"]),
        );

        assert_eq!(bridged.len(), MAX_BRIDGES_PER_WORD);
        // Strongest first, alphabetical among equals.
        assert_eq!(bridged[0].candidate, "one");
    }

    #[test]
    fn no_table_bridges_nothing() {
        let bridge = LexicalBridge::none();

        assert!(bridge.is_silent());
        assert!(bridge.is_empty());
        assert_eq!(bridge.similarity("valve", "valvula"), None);
        assert!(
            bridge
                .bridge(&words(&["valvula"]), &words(&["valve"]))
                .is_empty()
        );
    }

    #[test]
    fn an_empty_table_is_silent_and_valid() {
        let bridge = LexicalBridge::from_bytes(&table("empty", &[])).expect("the fixture is valid");

        assert!(bridge.is_silent());
        assert_eq!(bridge.provenance(), "empty");
    }

    #[test]
    fn a_table_that_does_not_account_for_its_bytes_is_refused() {
        let valid = table("toy", &[("valve", &[127, 0]), ("night", &[0, 127])]);

        assert!(LexicalBridge::from_bytes(b"KMPBRIDG").is_err());
        assert!(LexicalBridge::from_bytes(&valid[..valid.len() - 1]).is_err());
        let mut trailing = valid.clone();
        trailing.push(0);
        assert!(LexicalBridge::from_bytes(&trailing).is_err());
        let mut wrong_magic = valid.clone();
        wrong_magic[0] = b'X';
        assert!(LexicalBridge::from_bytes(&wrong_magic).is_err());
        let mut wrong_version = valid.clone();
        wrong_version[8] = 2;
        assert!(LexicalBridge::from_bytes(&wrong_version).is_err());
    }

    #[test]
    fn impossible_section_lengths_are_refused_before_allocation() {
        let mut impossible_offsets = Vec::new();
        impossible_offsets.extend_from_slice(super::super::lexical_bridge_table::MAGIC);
        impossible_offsets
            .extend_from_slice(&super::super::lexical_bridge_table::VERSION.to_le_bytes());
        impossible_offsets.extend_from_slice(&u16::MAX.to_le_bytes());
        impossible_offsets.extend_from_slice(&u32::MAX.to_le_bytes());
        impossible_offsets.extend_from_slice(&0u16.to_le_bytes());

        let error = LexicalBridge::from_owned_bytes(impossible_offsets)
            .expect_err("the declared offset section is absent");
        assert!(
            error.contains("truncated") || error.contains("too large"),
            "{error}"
        );

        let mut impossible_words = table("toy", &[("valve", &[127, 0])]);
        // Header and provenance occupy 21 bytes for `toy`; the second offset
        // starts four bytes later and claims a word section the file lacks.
        impossible_words[25..29].copy_from_slice(&u32::MAX.to_le_bytes());
        let error = LexicalBridge::from_owned_bytes(impossible_words)
            .expect_err("the declared word section is absent");
        assert!(error.contains("truncated"), "{error}");
    }

    #[test]
    fn malformed_utf8_is_refused_before_the_table_can_be_searched() {
        let mut bytes = table("toy", &[("a", &[127])]);
        // Header (18), provenance (3), two offsets (8), then the one word.
        bytes[29] = 0xff;

        let error = LexicalBridge::from_owned_bytes(bytes).expect_err("word is not UTF-8");
        assert!(error.contains("not UTF-8"), "{error}");
    }

    #[test]
    fn words_out_of_order_are_refused_because_lookup_would_lie() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(super::super::lexical_bridge_table::MAGIC);
        bytes.extend_from_slice(&super::super::lexical_bridge_table::VERSION.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        for offset in [0u32, 5, 10] {
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        bytes.extend_from_slice(b"valvenight");
        bytes.extend_from_slice(&[127, 0, 0, 127]);

        let error = LexicalBridge::from_bytes(&bytes).expect_err("unsorted words are refused");
        assert!(error.contains("not sorted"), "{error}");
    }

    #[test]
    fn a_zero_vector_is_unrelated_to_everything_rather_than_a_division_by_zero() {
        let bridge =
            LexicalBridge::from_bytes(&table("toy", &[("void", &[0, 0]), ("valve", &[127, 0])]))
                .expect("the fixture is valid");

        assert_eq!(bridge.similarity("void", "valve"), Some(0.0));
    }

    #[test]
    fn owned_and_borrowed_tables_return_identical_scores() {
        let bytes = table(
            "toy",
            &[
                ("night", &[0, 127, 0]),
                ("noche", &[10, 125, 0]),
                ("valve", &[127, 0, 0]),
                ("valvula", &[120, 40, 0]),
            ],
        );
        let borrowed = LexicalBridge::from_bytes(&bytes).expect("borrowed table");
        let owned = LexicalBridge::from_owned_bytes(bytes).expect("owned table");

        for (left, right) in [("night", "noche"), ("valve", "valvula"), ("night", "valve")] {
            assert_eq!(
                borrowed.similarity(left, right),
                owned.similarity(left, right)
            );
        }
    }

    #[test]
    fn lazy_norms_preserve_the_integer_cosine_bits_on_first_and_warm_use() {
        let negative_left = [-128, 127, -7, 0];
        let negative_right = [127, -128, 7, 1];
        let zero = [0, 0, 0, 0];
        let bridge = LexicalBridge::from_owned_bytes(table(
            "arithmetic-oracle",
            &[
                ("negative-left", &negative_left),
                ("negative-right", &negative_right),
                ("zero", &zero),
            ],
        ))
        .expect("arithmetic fixture");

        let expected = integer_cosine_oracle(&negative_left, &negative_right).to_bits();
        let first = bridge
            .similarity("negative-left", "negative-right")
            .expect("fixture words")
            .to_bits();
        let warm = bridge
            .similarity("negative-left", "negative-right")
            .expect("fixture words")
            .to_bits();
        assert_eq!(first, expected);
        assert_eq!(warm, expected);
        assert_eq!(
            bridge.similarity("negative-left", "zero"),
            Some(integer_cosine_oracle(&negative_left, &zero))
        );

        let near_left = [127, 0];
        let near_right = [57, 113];
        let near = LexicalBridge::from_owned_bytes(table(
            "threshold-oracle",
            &[("near-left", &near_left), ("near-right", &near_right)],
        ))
        .expect("threshold fixture");
        let expected = integer_cosine_oracle(&near_left, &near_right);
        assert!(
            expected >= MINIMUM_SIMILARITY && expected < 0.451,
            "{expected}"
        );
        assert_eq!(
            near.similarity("near-left", "near-right")
                .expect("fixture words")
                .to_bits(),
            expected.to_bits()
        );

        let max_dims = usize::from(u16::MAX);
        let max_left = vec![-128; max_dims];
        let max_right = (0..max_dims)
            .map(|index| if index % 2 == 0 { -128 } else { 127 })
            .collect::<Vec<i8>>();
        let maximum = LexicalBridge::from_owned_bytes(table(
            "maximum-dimensions",
            &[("max-left", &max_left), ("max-right", &max_right)],
        ))
        .expect("maximum format-v1 dimensions remain in bounds");
        let expected = integer_cosine_oracle(&max_left, &max_right).to_bits();
        assert_eq!(
            maximum
                .similarity("max-left", "max-right")
                .expect("fixture words")
                .to_bits(),
            expected
        );
    }

    #[test]
    fn cloned_tables_share_safe_concurrent_lazy_norms() {
        let left = [120, 40, -8, -128];
        let right = [127, 0, -4, -120];
        let expected = Some(integer_cosine_oracle(&left, &right));
        let bridge = LexicalBridge::from_owned_bytes(table(
            "concurrent-cold-norms",
            &[("cold-left", &left), ("cold-right", &right)],
        ))
        .expect("concurrency fixture");

        std::thread::scope(|scope| {
            for _ in 0..8 {
                let bridge = bridge.clone();
                scope.spawn(move || {
                    for _ in 0..1_000 {
                        assert_eq!(bridge.similarity("cold-left", "cold-right"), expected);
                        assert_eq!(bridge.similarity("void", "missing"), None);
                    }
                });
            }
        });
    }
}
