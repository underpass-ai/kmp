use serde::Deserialize;

use super::ask_judge_question::AskJudgeQuestion;

/// The file beside the store that opts an ask into its doubt band.
pub(super) const ASK_JUDGE_FILE: &str = "ask-judge.json";

/// The per-store opt-in for the ask doubt band (DESIGN L4 4f, option B). It
/// rides on the store's `typesafe.json`. Every field has a default, so `{}`
/// turns the band on with the thresholds measured on the development
/// corpora; B2 promotion stays off unless `promote` says otherwise.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AskJudgeConfig {
    /// B1: a cited memory the judge finds at least this likely not to
    /// answer leaves the core.
    #[serde(default = "default_veto_at")]
    veto_at: f64,
    /// B2 behind its own flag: admitted memories that passed the anchored
    /// gate may be promoted into the core.
    #[serde(default)]
    promote: bool,
    /// B2: how likely to answer a memory must be judged to be promoted.
    #[serde(default = "default_promote_at")]
    promote_at: f64,
    /// (iii): an answer whose first citation leads the second by less than
    /// this many tenths of a BM25 point is in doubt.
    #[serde(default = "default_margin_tenths")]
    margin_tenths: i64,
    #[serde(default)]
    question: AskJudgeQuestion,
    /// How much of each passage the judge reads.
    #[serde(default = "default_excerpt_chars")]
    excerpt_chars: usize,
}

fn default_veto_at() -> f64 {
    0.9
}

fn default_promote_at() -> f64 {
    0.9
}

/// Fixed on the development corpora (P10, `p10-prereg.v1`): J1 is highest at
/// a 2.0-point margin with `veto_at` 0.9 (synth, retrieval 53, negatives s7).
fn default_margin_tenths() -> i64 {
    20
}

fn default_excerpt_chars() -> usize {
    1_200
}

/// A probability as thousandths.
fn permille(probability: f64) -> u16 {
    (probability * 1_000.0).round() as u16
}

impl AskJudgeConfig {
    pub(super) fn validate(&self) -> Result<(), String> {
        for (name, value) in [("veto_at", self.veto_at), ("promote_at", self.promote_at)] {
            if !(value.is_finite() && value > 0.0 && value <= 1.0) {
                return Err(format!("ask-judge {name} must be in (0, 1]"));
            }
        }
        if self.promote && permille(self.veto_at) + permille(self.promote_at) <= 1_000 {
            return Err(
                "ask-judge veto_at plus promote_at must exceed 1, so no passage is both".into(),
            );
        }
        if !(0..=1_000).contains(&self.margin_tenths) {
            return Err("ask-judge margin_tenths must be between 0 and 1000".into());
        }
        if !(200..=2_000).contains(&self.excerpt_chars) {
            return Err("ask-judge excerpt_chars must be between 200 and 2000".into());
        }
        Ok(())
    }

    pub(super) fn veto_permille(&self) -> u16 {
        permille(self.veto_at)
    }

    /// `None` while B2 is off.
    pub(super) fn promote_permille(&self) -> Option<u16> {
        self.promote.then(|| permille(self.promote_at))
    }

    pub(super) fn margin_tenths(&self) -> i64 {
        self.margin_tenths
    }

    pub(super) fn question(&self) -> AskJudgeQuestion {
        self.question
    }

    pub(super) fn excerpt_chars(&self) -> usize {
        self.excerpt_chars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<AskJudgeConfig, String> {
        let config: AskJudgeConfig = serde_json::from_str(text).map_err(|e| e.to_string())?;
        config.validate().map(|()| config)
    }

    #[test]
    fn an_empty_file_turns_on_the_veto_and_leaves_promotion_off() {
        let config = parse("{}").expect("defaults");
        assert_eq!(config.veto_permille(), 900);
        assert_eq!(config.promote_permille(), None);
        assert_eq!(config.margin_tenths(), 20);
        assert_eq!(config.question(), AskJudgeQuestion::Noul);
        assert_eq!(config.excerpt_chars(), 1_200);
    }

    #[test]
    fn promotion_needs_its_flag_and_bars_that_exclude_each_other() {
        let config =
            parse(r#"{"promote":true,"promote_at":0.85,"veto_at":0.8,"question":"score"}"#)
                .expect("valid");
        assert_eq!(config.promote_permille(), Some(850));
        assert_eq!(config.question(), AskJudgeQuestion::Score);
        assert!(parse(r#"{"promote":true,"promote_at":0.5,"veto_at":0.5}"#).is_err());
        assert!(
            parse(r#"{"promote":false,"promote_at":0.5,"veto_at":0.5}"#).is_ok(),
            "without promotion the bars cannot meet"
        );
    }

    #[test]
    fn out_of_range_or_unknown_fields_are_refused() {
        for text in [
            r#"{"veto_at":0}"#,
            r#"{"veto_at":1.5}"#,
            r#"{"margin_tenths":-1}"#,
            r#"{"excerpt_chars":100}"#,
            r#"{"question":"choice"}"#,
            r#"{"model":"jev"}"#,
        ] {
            assert!(parse(text).is_err(), "accepted {text}");
        }
    }
}
