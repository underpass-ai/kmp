use std::path::Path;

use kmp_proto_mapping::v1beta1::AskGate;
use serde::Deserialize;

/// The file beside a store that opts it into the anchored ask gate.
pub(super) const ASK_GATE_FILE: &str = "ask-gate.json";

/// Per-store opt-in for the anchored decision of `kmp_ask`:
/// `{"mode":"anchored","partial":true}`. Absent, ask answers exactly as it
/// did; unreadable or unknown, it is ignored and reported so.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AskGateConfig {
    mode: String,
    /// Whether an enumerative question answered in part is PARTIAL rather
    /// than UNKNOWN.
    #[serde(default = "partial_by_default")]
    partial: bool,
}

fn partial_by_default() -> bool {
    true
}

impl AskGateConfig {
    /// The gate this store opted into: `None` without the file, an error
    /// naming why a present file cannot apply.
    pub(super) fn load(data_dir: &Path) -> Result<Option<AskGate>, String> {
        let path = data_dir.join(ASK_GATE_FILE);
        if !path.is_file() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{ASK_GATE_FILE} is unreadable: {error}"))?;
        Self::parse(&text).map(Some)
    }

    fn parse(text: &str) -> Result<AskGate, String> {
        let config: Self = serde_json::from_str(text)
            .map_err(|error| format!("{ASK_GATE_FILE} is not a gate configuration: {error}"))?;
        match config.mode.as_str() {
            "anchored" => Ok(AskGate::anchored(config.partial)),
            other => Err(format!(
                "{ASK_GATE_FILE} names mode `{other}`; this kmp-mcp knows `anchored`"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_anchored_mode_is_accepted() {
        assert_eq!(
            AskGateConfig::parse(r#"{"mode":"anchored","partial":true}"#),
            Ok(AskGate::anchored(true))
        );
        assert_eq!(
            AskGateConfig::parse(r#"{"mode":"anchored","partial":false}"#),
            Ok(AskGate::anchored(false))
        );
        assert_eq!(
            AskGateConfig::parse(r#"{"mode":"anchored"}"#),
            Ok(AskGate::anchored(true))
        );
        assert!(AskGateConfig::parse(r#"{"mode":"judged"}"#).is_err());
        assert!(AskGateConfig::parse(r#"{"mode":"anchored","tau":3}"#).is_err());
        assert!(AskGateConfig::parse("not json").is_err());
    }

    #[test]
    fn a_store_without_the_file_has_no_gate() {
        let dir = std::env::temp_dir().join(format!("kmp-ask-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        assert_eq!(AskGateConfig::load(&dir), Ok(None));
        std::fs::write(dir.join(ASK_GATE_FILE), r#"{"mode":"anchored"}"#).expect("write");
        assert_eq!(AskGateConfig::load(&dir), Ok(Some(AskGate::anchored(true))));
        std::fs::write(dir.join(ASK_GATE_FILE), "{").expect("write");
        assert!(AskGateConfig::load(&dir).is_err());
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
