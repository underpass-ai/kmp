//! The corridor of a goal path search as an evaluation arm (DESIGN L7).
//!
//! `KMP_EVAL_PATHS_CORRIDOR=on|off` writes `curate.json` with that
//! `paths_corridor` into the judged store a scorecard seeds; unset, the
//! store's default stands. It is how the corridor was measured against the
//! near-the-ends filter it replaces, over the same recorded judgements.

/// The `curate.json` body of the evaluation arm, if one is named.
pub fn paths_corridor_eval_arm() -> Option<String> {
    arm(std::env::var("KMP_EVAL_PATHS_CORRIDOR").ok().as_deref())
}

fn arm(value: Option<&str>) -> Option<String> {
    value.map(|value| {
        assert!(
            matches!(value, "on" | "off"),
            "KMP_EVAL_PATHS_CORRIDOR is `on` or `off`"
        );
        serde_json::json!({ "paths_corridor": value }).to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arm_names_the_corridor_or_leaves_the_store_default() {
        assert_eq!(arm(None), None);
        assert_eq!(
            arm(Some("off")).as_deref(),
            Some(r#"{"paths_corridor":"off"}"#)
        );
    }
}
