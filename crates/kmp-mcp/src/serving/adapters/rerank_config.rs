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
    /// The margin gate of Ask re-ranking (DESIGN L4 4c): an Ask whose first
    /// lexical candidate leads the second by at least this many tenths of a
    /// content-score point, with `High` confidence, sends no request. Absent,
    /// the measured default; `null` turns the gate off. Wake focus ignores it.
    #[serde(default = "measured_margin")]
    pub margin_tenths: Option<i64>,
}

/// The margin gate's threshold, fixed on three recorded samples of the
/// judged corpora (`docs/development/jev-evaluation.md`, P9).
pub(super) const MARGIN_TENTHS: i64 = 0;

fn full_excerpt() -> usize {
    2_000
}

fn measured_margin() -> Option<i64> {
    Some(MARGIN_TENTHS)
}

impl RerankConfig {
    pub(super) fn validate(&self) -> Result<(usize, usize), String> {
        if !(1..=400).contains(&self.pool_size) {
            return Err("rerank pool_size must be between 1 and 400".into());
        }
        if !(200..=2_000).contains(&self.excerpt_chars) {
            return Err("rerank excerpt_chars must be between 200 and 2000".into());
        }
        if self
            .margin_tenths
            .is_some_and(|tenths| !(0..=10_000).contains(&tenths))
        {
            return Err("rerank margin_tenths must be between 0 and 10000, or null".into());
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
            margin_tenths: Some(MARGIN_TENTHS),
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

    #[test]
    fn the_margin_gate_is_on_at_the_measured_threshold_unless_turned_off() {
        let parse = |text: &str| serde_json::from_str::<RerankConfig>(text).expect("parses");
        assert_eq!(
            parse(r#"{"pool_size":8}"#).margin_tenths,
            Some(MARGIN_TENTHS)
        );
        assert_eq!(
            parse(r#"{"pool_size":8,"margin_tenths":null}"#).margin_tenths,
            None
        );
        assert_eq!(
            parse(r#"{"pool_size":8,"margin_tenths":5}"#).margin_tenths,
            Some(5)
        );
        assert!(
            parse(r#"{"pool_size":8,"margin_tenths":-1}"#)
                .validate()
                .is_err()
        );
    }
}
