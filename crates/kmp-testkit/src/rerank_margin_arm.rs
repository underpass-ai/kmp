//! The margin gate of Ask re-ranking as an evaluation arm.
//!
//! `KMP_EVAL_RERANK_MARGIN=<tenths>|off` writes that threshold into every
//! `rerank.json` a scorecard seeds; unset, the store's default stands. It is
//! how the threshold was fixed (DESIGN L4 4c): the same recorded judgements,
//! read with the gate at several thresholds and without it.

use serde_json::Value;

/// `rerank` (a `rerank.json` body) with the evaluation threshold, if any.
pub fn rerank_with_eval_margin(rerank: &str) -> String {
    with_margin(
        rerank,
        std::env::var("KMP_EVAL_RERANK_MARGIN").ok().as_deref(),
    )
}

fn with_margin(rerank: &str, margin: Option<&str>) -> String {
    let Some(margin) = margin else {
        return rerank.to_string();
    };
    let mut config: Value = serde_json::from_str(rerank).expect("a rerank.json body is JSON");
    config["margin_tenths"] = match margin {
        "off" => Value::Null,
        tenths => Value::from(
            tenths
                .parse::<i64>()
                .expect("KMP_EVAL_RERANK_MARGIN is tenths or `off`"),
        ),
    };
    config.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arm_sets_the_threshold_or_turns_the_gate_off_and_otherwise_leaves_the_body() {
        let body = r#"{"pool_size":40}"#;
        assert_eq!(with_margin(body, None), body);
        assert_eq!(
            with_margin(body, Some("25")),
            r#"{"margin_tenths":25,"pool_size":40}"#
        );
        assert_eq!(
            with_margin(body, Some("off")),
            r#"{"margin_tenths":null,"pool_size":40}"#
        );
    }
}
