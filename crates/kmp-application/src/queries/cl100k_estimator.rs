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

    fn name(&self) -> &str {
        "cl100k_base"
    }
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
}
