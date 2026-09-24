use serde::Deserialize;

/// Explicit per-store opt-in for Ask re-ranking. It rides on the store's
/// TypeSafe opt-in and adds only how many passages one Ask may send.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RerankConfig {
    pub pool_size: usize,
}

impl RerankConfig {
    pub(super) fn validate(&self) -> Result<usize, String> {
        if (1..=40).contains(&self.pool_size) {
            Ok(self.pool_size)
        } else {
            Err("rerank pool_size must be between 1 and 40".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pool_is_bounded_and_nothing_else_is_accepted() {
        assert_eq!(RerankConfig { pool_size: 40 }.validate(), Ok(40));
        assert!(RerankConfig { pool_size: 0 }.validate().is_err());
        assert!(RerankConfig { pool_size: 41 }.validate().is_err());
        assert!(serde_json::from_str::<RerankConfig>(r#"{"pool_size":8,"model":"x"}"#).is_err());
    }
}
