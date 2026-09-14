use std::sync::OnceLock;

use kmp_domain::TokenEstimator;

/// Estimates tokens using OpenAI's `cl100k_base` BPE encoding.
///
/// This is the standard tokenizer used by GPT-4, GPT-4o, and Claude-family
/// models. Using a real BPE tokenizer makes token budget enforcement
/// defensible and reproducible across implementations.
///
/// Loading the vocabulary is the expensive part — a hundred thousand merges
/// parsed from text — and it was paid on every call that needed an estimate:
/// once to render a bundle nobody would read, once more to project the
/// response. On a store of three entries that was most of an `ask`. The
/// vocabulary is immutable, so one process reads it once and every caller
/// shares it.
pub struct Cl100kEstimator {
    bpe: tiktoken_rs::CoreBPE,
}

impl Cl100kEstimator {
    /// Loads the vocabulary. Callers on a request path want `shared`; this
    /// exists for tests and for the one load `shared` performs.
    pub fn new() -> Self {
        Self {
            bpe: tiktoken_rs::cl100k_base().expect("cl100k_base vocabulary should load"),
        }
    }

    /// The process-wide estimator, loaded on first use.
    pub fn shared() -> &'static Self {
        static SHARED: OnceLock<Cl100kEstimator> = OnceLock::new();
        SHARED.get_or_init(Self::new)
    }
}

impl Default for Cl100kEstimator {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenEstimator for Cl100kEstimator {
    fn estimate_tokens(&self, text: &str) -> u32 {
        self.bpe.encode_ordinary(text).len() as u32
    }

    fn estimate_token_records(&self, records: &mut dyn Iterator<Item = String>) -> u32 {
        let Some(mut pending) = records.next() else {
            return 0;
        };
        let mut total = 0u64;
        for record in records {
            if can_flush_record(&pending, &record) {
                total += self.estimate_tokens(&pending) as u64;
                pending = record;
            } else {
                pending.push_str(&record);
            }
        }
        total += self.estimate_tokens(&pending) as u64;
        total as u32
    }

    fn name(&self) -> &str {
        "cl100k_base"
    }
}

/// `cl100k_base`'s public regex separates trailing whitespace from the next
/// ASCII word. Raw-dump records end in LF, so a following ASCII letter starts
/// a new regex match and cannot merge with the preceding record. Every other
/// boundary is retained in `pending` to preserve the full-string result.
fn can_flush_record(previous: &str, next: &str) -> bool {
    previous.ends_with('\n') && next.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
}

#[cfg(test)]
mod tests {
    use kmp_domain::TokenEstimator;

    use super::Cl100kEstimator;

    #[test]
    fn returns_expected_counts_for_known_inputs() {
        let estimator = Cl100kEstimator::new();
        assert_eq!(estimator.estimate_tokens("hello world"), 2);
        assert_eq!(estimator.name(), "cl100k_base");
    }

    #[test]
    fn handles_empty_input() {
        let estimator = Cl100kEstimator::new();
        assert_eq!(estimator.estimate_tokens(""), 0);
    }

    #[test]
    fn the_shared_estimator_is_one_load_and_counts_the_same() {
        let first = Cl100kEstimator::shared();
        let second = Cl100kEstimator::shared();

        assert!(std::ptr::eq(first, second));
        assert_eq!(
            first.estimate_tokens("hello world"),
            Cl100kEstimator::new().estimate_tokens("hello world")
        );
    }

    #[test]
    fn record_boundaries_preserve_cl100k_count_for_recall_text() {
        let records = vec![
            "Node: case. Kind: case. Summary: café 😀 with \\\"quotes\\\" and \\\\slash.\n".to_string(),
            "Node: node. Kind: claim. Summary: line one\nline two. Detail: body.\n".to_string(),
            "Relationship: case connects to node via SUPPORTS. Semantic class: causal. Rationale: why. Decision: d-1.\n".to_string(),
        ];
        let joined = records.concat();
        let estimator = Cl100kEstimator::shared();
        let mut iter = records.into_iter();
        assert_eq!(
            estimator.estimate_token_records(&mut iter),
            estimator.estimate_tokens(&joined)
        );

        let unsafe_records = vec![
            "hel".to_string(),
            "lo".to_string(),
            "'s".to_string(),
            " 😀".to_string(),
            "\n".to_string(),
            "123".to_string(),
            String::new(),
            "é".repeat(128),
        ];
        let unsafe_joined = unsafe_records.concat();
        let mut unsafe_iter = unsafe_records.into_iter();
        assert_eq!(
            estimator.estimate_token_records(&mut unsafe_iter),
            estimator.estimate_tokens(&unsafe_joined)
        );

        for previous in ["\n", "\r\n", " \n", "😀.\n", "a\n\n"] {
            for next in ["Node: next", "Relationship: next", "alpha", "123", "!", ""] {
                let records = vec![previous.to_string(), next.to_string()];
                let joined = records.concat();
                let mut iter = records.into_iter();
                assert_eq!(
                    estimator.estimate_token_records(&mut iter),
                    estimator.estimate_tokens(&joined),
                    "boundary {previous:?} + {next:?}"
                );
            }
        }
    }
}
