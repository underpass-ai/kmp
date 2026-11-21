use serde::Deserialize;

/// The file beside the store that opts an ask into its cross-language judge.
pub(super) const ASK_CROSS_LANGUAGE_FILE: &str = "ask-cross-language.json";

/// The per-store opt-in for Jev as the cross-language judge of `kmp_ask`.
/// It rides on the store's `typesafe.json`. Both effects are off unless the
/// file turns them on, each on its own:
///
/// - `veto`: an answer that stands only because the lexical bridge
///   translated what its citations state in another language is asked
///   about; a citation stays only when Jev finds it at least `keep_at`
///   likely to state the answer. One judged unsettled or not stating it
///   leaves the core, and the answer falls back to what it is without it.
/// - `rescue`: an UNKNOWN to a question with no required anchor, for which
///   the bridge reached memories written in another language, is asked
///   about up to eight of them and whether the question can be answered
///   from them at all; the likeliest passage is cited only when it and that
///   companion judgement both reach `rescue_at`. Never `high`.
///
/// A Noul reads yes at or above its line, no at or below 0.3, and is
/// unsettled in between (`StatedReading` in kmp-proto-mapping holds the
/// table). The lines are TypeSafe's starting yes line (0.7) and, for an
/// answer created on a judgement alone, the stricter 0.9 of probe F3; see
/// `docs/embedded/languages.md` for what they were measured to do.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AskCrossLanguageConfig {
    #[serde(default)]
    veto: bool,
    #[serde(default)]
    rescue: bool,
    /// How likely to state the answer a translated citation must be judged
    /// to stay in the core.
    #[serde(default = "default_keep_at")]
    keep_at: f64,
    /// How likely the rescued passage, and the companion, must be judged.
    #[serde(default = "default_rescue_at")]
    rescue_at: f64,
    /// How much of each passage Jev reads.
    #[serde(default = "default_excerpt_chars")]
    excerpt_chars: usize,
}

fn default_keep_at() -> f64 {
    0.7
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
        // At or below 0.3 a Noul reads no, so a line there could never
        // read yes.
        for (name, value) in [("keep_at", self.keep_at), ("rescue_at", self.rescue_at)] {
            if !(value.is_finite() && value > 0.3 && value <= 1.0) {
                return Err(format!("ask-cross-language {name} must be in (0.3, 1]"));
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

    pub(super) fn keep_permille(&self) -> u16 {
        permille(self.keep_at)
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
        assert_eq!(veto.keep_permille(), 700);
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
            r#"{"veto":true,"keep_at":0.3}"#,
            r#"{"veto":true,"veto_at":0.5}"#,
            r#"{"rescue":true,"rescue_at":1.5}"#,
            r#"{"veto":true,"excerpt_chars":100}"#,
            r#"{"veto":true,"model":"jev"}"#,
        ] {
            assert!(parse(text).is_err(), "accepted {text}");
        }
    }
}
