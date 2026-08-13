//! Raw document dump baseline for token efficiency comparison.
//!
//! Uses `BundleQualityMetrics::compute()` from the domain layer via
//! `seed_to_bundle()` — single source of truth for raw token counting.

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_domain::BundleQualityMetrics;

use crate::dataset_generator::GeneratedSeed;
use crate::seed_to_bundle::seed_to_bundle;

/// Count raw equivalent tokens for a seed using the domain VO.
///
/// This delegates to `BundleQualityMetrics::compute()` through the
/// `seed_to_bundle` mapper — identical to what the kernel computes.
pub fn count_raw_tokens(seed: &GeneratedSeed) -> u32 {
    let bundle = seed_to_bundle(seed);
    let estimator = Cl100kEstimator::new();
    BundleQualityMetrics::compute(&bundle, 0, &estimator).raw_equivalent_tokens()
}

/// Count tokens in arbitrary text using cl100k_base.
pub fn count_tokens(text: &str) -> usize {
    let bpe = tiktoken_rs::cl100k_base().expect("cl100k_base tokenizer should load");
    bpe.encode_ordinary(text).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset_generator::{Domain, GraphSeedConfig, generate_seed};

    #[test]
    fn raw_tokens_is_positive() {
        let seed = generate_seed(GraphSeedConfig::micro(Domain::Operations));
        let tokens = count_raw_tokens(&seed);
        assert!(
            tokens > 50,
            "raw tokens should be significant, got {tokens}"
        );
    }

    #[test]
    fn meso_raw_tokens_is_substantial() {
        let seed = generate_seed(GraphSeedConfig::meso(Domain::Operations));
        let tokens = count_raw_tokens(&seed);
        assert!(tokens > 200, "meso raw tokens should be >200, got {tokens}");
    }

    #[test]
    fn raw_tokens_increases_with_scale() {
        let micro = generate_seed(GraphSeedConfig::micro(Domain::Operations));
        let meso = generate_seed(GraphSeedConfig::meso(Domain::Operations));
        assert!(
            count_raw_tokens(&meso) > count_raw_tokens(&micro),
            "meso should have more raw tokens than micro"
        );
    }
}
