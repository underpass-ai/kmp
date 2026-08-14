use kmp_domain::TokenEstimator;

/// Estimates tokens using OpenAI's `cl100k_base` BPE encoding.
///
/// This is the standard tokenizer used by GPT-4, GPT-4o, and Claude-family
/// models. Using a real BPE tokenizer makes token budget enforcement
/// defensible and reproducible across implementations.
pub struct Cl100kEstimator {
    bpe: tiktoken_rs::CoreBPE,
}

impl Cl100kEstimator {
    pub fn new() -> Self {
        Self {
            bpe: tiktoken_rs::cl100k_base().expect("cl100k_base vocabulary should load"),
        }
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
}
