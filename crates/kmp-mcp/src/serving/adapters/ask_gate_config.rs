use std::path::Path;

use kmp_proto_mapping::v1beta1::{AskGate, ConfidenceCalibration};
use serde::Deserialize;

/// The file beside a store that chooses its anchored ask gate.
pub(super) const ASK_GATE_FILE: &str = "ask-gate.json";

/// Per-store choice of the anchored decision of `kmp_ask`:
/// `{"mode":"anchored","partial":true}` is the gate, `{"mode":"off"}` opts
/// out of it (ask answers as v0.23.0 did). `"successor_core":true` turns on
/// the measured lifecycle variant (P7): the current head of a replaced
/// memory that named the principal anchor may be cited for it.
/// `"confidence_calibration":"shipped"` states `proof.confidence` through the
/// shipped calibration table (P16), and an inline table object measures a
/// candidate one; off without the key. `"attribute_check":true` holds an
/// unanchored `high` to a citation that states the asked attribute (off by
/// default); `"expansion_rescue_focus":false` measures P15's expansion rescue
/// without the strict focus (on by default). Absent, the store gets
/// [`AskGate::STORE_DEFAULT`] (the gate, with PARTIAL); unreadable or
/// unknown, it is ignored and reported so, and the default applies.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AskGateConfig {
    mode: String,
    /// Whether an enumerative question answered in part is PARTIAL rather
    /// than UNKNOWN.
    #[serde(default = "partial_by_default")]
    partial: bool,
    /// Whether a question about now may cite the standing successor of a
    /// replaced memory that named its principal anchor. Off by default.
    #[serde(default)]
    successor_core: bool,
    /// `"shipped"`, or an inline calibration table. Off by default.
    #[serde(default)]
    confidence_calibration: Option<serde_json::Value>,
    /// Whether an unanchored `high` needs a citation stating the asked
    /// attribute. Off by default.
    #[serde(default)]
    attribute_check: bool,
    /// Whether an expansion rescue must answer the strict focus. On by
    /// default.
    #[serde(default = "focus_by_default")]
    expansion_rescue_focus: bool,
}

fn focus_by_default() -> bool {
    true
}

fn partial_by_default() -> bool {
    true
}

impl AskGateConfig {
    /// The gate this store reads with: the default without the file, what
    /// the file says with it, an error naming why a present file cannot
    /// apply.
    pub(super) fn load(data_dir: &Path) -> Result<Option<AskGate>, String> {
        let path = data_dir.join(ASK_GATE_FILE);
        if !path.is_file() {
            return Ok(AskGate::STORE_DEFAULT);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{ASK_GATE_FILE} is unreadable: {error}"))?;
        Self::parse(&text)
    }

    fn parse(text: &str) -> Result<Option<AskGate>, String> {
        let config: Self = serde_json::from_str(text)
            .map_err(|error| format!("{ASK_GATE_FILE} is not a gate configuration: {error}"))?;
        match config.mode.as_str() {
            "anchored" => Ok(Some(
                AskGate::anchored(config.partial)
                    .with_successor_core(config.successor_core)
                    .with_attribute_check(config.attribute_check)
                    .with_expansion_focus(config.expansion_rescue_focus)
                    .with_confidence_calibration(calibration(
                        config.confidence_calibration.as_ref(),
                    )?),
            )),
            "off" => Ok(None),
            other => Err(format!(
                "{ASK_GATE_FILE} names mode `{other}`; this kmp-mcp knows `anchored` and `off`"
            )),
        }
    }
}

/// The calibration table a gate configuration names: the shipped one, or an
/// inline table read once for the life of the process.
fn calibration(
    value: Option<&serde_json::Value>,
) -> Result<Option<&'static ConfidenceCalibration>, String> {
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(name)) if name == "shipped" => {
            Ok(Some(ConfidenceCalibration::shipped()))
        }
        Some(table @ serde_json::Value::Object(_)) => {
            let table = ConfidenceCalibration::parse(&table.to_string())
                .map_err(|error| format!("{ASK_GATE_FILE} confidence_calibration: {error}"))?;
            Ok(Some(Box::leak(Box::new(table))))
        }
        Some(other) => Err(format!(
            "{ASK_GATE_FILE} confidence_calibration is `{other}`; this kmp-mcp knows \"shipped\" or an inline table"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_anchored_and_off_modes_are_accepted() {
        assert_eq!(
            AskGateConfig::parse(r#"{"mode":"anchored","partial":true}"#),
            Ok(Some(AskGate::anchored(true)))
        );
        assert_eq!(
            AskGateConfig::parse(r#"{"mode":"anchored","partial":false}"#),
            Ok(Some(AskGate::anchored(false)))
        );
        assert_eq!(
            AskGateConfig::parse(r#"{"mode":"anchored"}"#),
            Ok(Some(AskGate::anchored(true)))
        );
        assert_eq!(AskGateConfig::parse(r#"{"mode":"off"}"#), Ok(None));
        assert_eq!(
            AskGateConfig::parse(r#"{"mode":"anchored","successor_core":true}"#),
            Ok(Some(AskGate::anchored(true).with_successor_core(true)))
        );
        assert!(
            !AskGateConfig::parse(r#"{"mode":"anchored"}"#)
                .expect("parses")
                .expect("a gate")
                .admits_successor_to_core(),
            "the lifecycle variant is off unless a store turns it on"
        );
        assert!(AskGateConfig::parse(r#"{"mode":"judged"}"#).is_err());
        assert!(AskGateConfig::parse(r#"{"mode":"anchored","tau":3}"#).is_err());
        assert!(AskGateConfig::parse("not json").is_err());
    }

    #[test]
    fn the_attribute_check_and_the_expansion_focus_variants_are_off_by_default() {
        let plain = AskGateConfig::parse(r#"{"mode":"anchored"}"#)
            .expect("parses")
            .expect("a gate");
        assert!(!plain.checks_attribute());
        assert!(plain.requires_expansion_focus());
        let variant = AskGateConfig::parse(
            r#"{"mode":"anchored","attribute_check":true,"expansion_rescue_focus":false}"#,
        )
        .expect("parses")
        .expect("a gate");
        assert!(variant.checks_attribute());
        assert!(!variant.requires_expansion_focus());
    }

    #[test]
    fn confidence_calibration_is_off_unless_named() {
        let plain = AskGateConfig::parse(r#"{"mode":"anchored"}"#)
            .expect("parses")
            .expect("a gate");
        assert!(plain.confidence_calibration().is_none());
        let shipped =
            AskGateConfig::parse(r#"{"mode":"anchored","confidence_calibration":"shipped"}"#)
                .expect("parses")
                .expect("a gate");
        assert_eq!(
            shipped.confidence_calibration(),
            Some(ConfidenceCalibration::shipped())
        );
        let inline = AskGateConfig::parse(
            r#"{"mode":"anchored","confidence_calibration":{"version":"t","demote_high":[{"id":"n","negated_anchor":true}]}}"#,
        )
        .expect("parses")
        .expect("a gate");
        assert_eq!(
            inline
                .confidence_calibration()
                .map(ConfidenceCalibration::version),
            Some("t")
        );
        assert!(
            AskGateConfig::parse(r#"{"mode":"anchored","confidence_calibration":"fitted"}"#)
                .is_err()
        );
        assert!(
            AskGateConfig::parse(
                r#"{"mode":"anchored","confidence_calibration":{"version":"t","demote_high":[{"id":"n","weight":1}]}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn a_store_without_the_file_has_the_default_gate() {
        // The gate is the default since B-real was judged (2026-09-26); this
        // pins it, so turning it off is a reviewed change of
        // `AskGate::STORE_DEFAULT` and of this.
        assert_eq!(AskGate::STORE_DEFAULT, Some(AskGate::anchored(true)));
        let dir = std::env::temp_dir().join(format!("kmp-ask-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        assert_eq!(AskGateConfig::load(&dir), Ok(AskGate::STORE_DEFAULT));
        std::fs::write(
            dir.join(ASK_GATE_FILE),
            r#"{"mode":"anchored","partial":false}"#,
        )
        .expect("write");
        assert_eq!(
            AskGateConfig::load(&dir),
            Ok(Some(AskGate::anchored(false)))
        );
        std::fs::write(dir.join(ASK_GATE_FILE), r#"{"mode":"off"}"#).expect("write");
        assert_eq!(AskGateConfig::load(&dir), Ok(None));
        std::fs::write(dir.join(ASK_GATE_FILE), "{").expect("write");
        assert!(AskGateConfig::load(&dir).is_err());
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
