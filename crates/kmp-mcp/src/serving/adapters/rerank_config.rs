use serde::Deserialize;

/// Explicit per-store opt-in for Ask re-ranking. It rides on the store's
/// TypeSafe opt-in and adds how many admitted passages one Ask may send and
/// how much of each. A wide pool of short excerpts reaches answers the
/// lexical ranker never lists; the cost is tokens and latency per Ask.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RerankConfig {
    pub pool_size: usize,
    #[serde(default = "full_excerpt")]
    pub excerpt_chars: usize,
}

fn full_excerpt() -> usize {
    2_000
}

impl RerankConfig {
    pub(super) fn validate(&self) -> Result<(usize, usize), String> {
        if !(1..=400).contains(&self.pool_size) {
            return Err("rerank pool_size must be between 1 and 400".into());
        }
        if !(200..=2_000).contains(&self.excerpt_chars) {
            return Err("rerank excerpt_chars must be between 200 and 2000".into());
        }
        Ok((self.pool_size, self.excerpt_chars))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pool_is_bounded_and_nothing_else_is_accepted() {
        let config = |pool_size, excerpt_chars| RerankConfig {
            pool_size,
            excerpt_chars,
        };
        assert_eq!(config(40, 2_000).validate(), Ok((40, 2_000)));
        assert_eq!(config(400, 300).validate(), Ok((400, 300)));
        assert!(config(0, 2_000).validate().is_err());
        assert!(config(401, 2_000).validate().is_err());
        assert!(config(40, 100).validate().is_err());
        let parsed: RerankConfig = serde_json::from_str(r#"{"pool_size":8}"#).expect("default");
        assert_eq!(parsed.excerpt_chars, 2_000);
        assert!(serde_json::from_str::<RerankConfig>(r#"{"pool_size":8,"model":"x"}"#).is_err());
    }
}
