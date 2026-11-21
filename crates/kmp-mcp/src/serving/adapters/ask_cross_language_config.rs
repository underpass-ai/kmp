use serde::Deserialize;

/// The file beside the store that opts an ask into its cross-language judge.
pub(super) const ASK_CROSS_LANGUAGE_FILE: &str = "ask-cross-language.json";

/// The per-store opt-in for Jev as the cross-language judge of `kmp_ask`.
/// It rides on the store's `typesafe.json`. Both effects are off unless the
/// file turns them on, each on its own:
///
/// - `veto`: an answer that stands only because the lexical bridge
///   translated what its citations state in another language is asked
///   about; a citation Jev finds at least `veto_at` likely not to state the
///   answer leaves the core, and the answer falls back to what it is
///   without it.
/// - `rescue`: an UNKNOWN to a question with no required anchor, for which
///   the bridge reached memories written in another language, is asked
///   about up to eight of them and whether the question can be answered
///   from them at all; the likeliest passage is cited only when it and that
///   companion judgement both reach `rescue_at`. Never `high`.
///
/// The lines were fixed before the measurement
/// (`docs/embedded/languages.md`); `rescue_at` is the one the measurement
/// supports.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AskCrossLanguageConfig {
    #[serde(default)]
    veto: bool,
    #[serde(default)]
    rescue: bool,
    /// How likely not to state the answer a translated citation must be
    /// judged to leave the core.
    #[serde(default = "default_veto_at")]
    veto_at: f64,
    /// How likely the rescued passage, and the companion, must be judged.
    #[serde(default = "default_rescue_at")]
    rescue_at: f64,
    /// How much of each passage Jev reads.
    #[serde(default = "default_excerpt_chars")]
    excerpt_chars: usize,
}

fn default_veto_at() -> f64 {
    0.5
}

fn default_rescue_at() -> f64 {
    0.9
}

fn default_excerpt_chars() -> usize {
    1_200
}

/// A probability as thousandths.
fn permille(probability: f64) -> u16 {
    (probability * 1_000.0).round() as u16
}

impl AskCrossLanguageConfig {
    pub(super) fn validate(&self) -> Result<(), String> {
        if !self.veto && !self.rescue {
            return Err("ask-cross-language turns on neither veto nor rescue".into());
        }
        for (name, value) in [("veto_at", self.veto_at), ("rescue_at", self.rescue_at)] {
            if !(value.is_finite() && value > 0.0 && value <= 1.0) {
                return Err(format!("ask-cross-language {name} must be in (0, 1]"));
            }
        }
        if !(200..=2_000).contains(&self.excerpt_chars) {
            return Err("ask-cross-language excerpt_chars must be between 200 and 2000".into());
        }
        Ok(())
    }

    pub(super) fn veto(&self) -> bool {
        self.veto
    }

    pub(super) fn rescue(&self) -> bool {
        self.rescue
    }

    pub(super) fn veto_permille(&self) -> u16 {
        permille(self.veto_at)
    }

    pub(super) fn rescue_permille(&self) -> u16 {
        permille(self.rescue_at)
    }

    pub(super) fn excerpt_chars(&self) -> usize {
        self.excerpt_chars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<AskCrossLanguageConfig, String> {
        let config: AskCrossLanguageConfig =
            serde_json::from_str(text).map_err(|e| e.to_string())?;
        config.validate().map(|()| config)
    }

    #[test]
    fn each_effect_is_off_unless_named_and_the_lines_have_defaults() {
        let veto = parse(r#"{"veto":true}"#).expect("veto only");
        assert!(veto.veto() && !veto.rescue());
        assert_eq!(veto.veto_permille(), 500);
        assert_eq!(veto.rescue_permille(), 900);
        assert_eq!(veto.excerpt_chars(), 1_200);
        let rescue = parse(r#"{"rescue":true,"rescue_at":0.7}"#).expect("rescue only");
        assert!(!rescue.veto() && rescue.rescue());
        assert_eq!(rescue.rescue_permille(), 700);
    }

    #[test]
    fn a_file_that_turns_nothing_on_or_is_out_of_range_is_refused() {
        for text in [
            "{}",
            r#"{"veto":false,"rescue":false}"#,
            r#"{"veto":true,"veto_at":0}"#,
            r#"{"rescue":true,"rescue_at":1.5}"#,
            r#"{"veto":true,"excerpt_chars":100}"#,
            r#"{"veto":true,"model":"jev"}"#,
        ] {
            assert!(parse(text).is_err(), "accepted {text}");
        }
    }
}
