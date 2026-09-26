use std::path::Path;

use serde::Deserialize;

/// The file beside a store that tunes or turns off its verdict book.
pub(super) const VERDICT_BOOK_CONFIG_FILE: &str = "judgement-book.json";
/// The book itself, beside the store's configuration and outside `store/`:
/// the kernel refuses files it does not own there, and older binaries must
/// keep opening the store. Local to the machine; `kmp:save` never carries it
/// and deleting it only costs the verdicts it held.
pub(super) const VERDICT_BOOK_FILE: &str = "judgements.sqlite3";

const MIB: u64 = 1024 * 1024;
const DEFAULT_MAX_BYTES: u64 = 64 * MIB;
const MIN_MAX_BYTES: u64 = MIB;
const MAX_MAX_BYTES: u64 = 4096 * MIB;

/// `{"mode":"on"|"off","max_bytes":N}`, both optional. Verdicts beyond
/// `max_bytes` are collected least recently used first; the file itself can
/// never grow past twice that.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VerdictBookConfig {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    max_bytes: Option<u64>,
}

impl VerdictBookConfig {
    /// `Ok(None)` when the store turned the book off; the byte budget
    /// otherwise. An unreadable or invalid file is an error that names why,
    /// and leaves the store without a book.
    pub(super) fn load(data_dir: &Path) -> Result<Option<u64>, String> {
        let path = data_dir.join(VERDICT_BOOK_CONFIG_FILE);
        let config = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= 8192 => serde_json::from_slice::<Self>(&bytes)
                .map_err(|_| format!("invalid {VERDICT_BOOK_CONFIG_FILE}"))?,
            Ok(_) => return Err(format!("{VERDICT_BOOK_CONFIG_FILE} exceeds 8192 bytes")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(_) => return Err(format!("cannot read {VERDICT_BOOK_CONFIG_FILE}")),
        };
        config.validate()
    }

    fn validate(&self) -> Result<Option<u64>, String> {
        match self.mode.as_deref() {
            None | Some("on") => {}
            Some("off") => return Ok(None),
            Some(_) => return Err(format!("{VERDICT_BOOK_CONFIG_FILE} mode must be on or off")),
        }
        let max_bytes = self.max_bytes.unwrap_or(DEFAULT_MAX_BYTES);
        if !(MIN_MAX_BYTES..=MAX_MAX_BYTES).contains(&max_bytes) {
            return Err(format!(
                "{VERDICT_BOOK_CONFIG_FILE} max_bytes must be between 1 MiB and 4 GiB"
            ));
        }
        Ok(Some(max_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(text: Option<&str>) -> Result<Option<u64>, String> {
        let dir = tempfile::tempdir().expect("dir");
        if let Some(text) = text {
            std::fs::write(dir.path().join(VERDICT_BOOK_CONFIG_FILE), text).expect("write");
        }
        VerdictBookConfig::load(dir.path())
    }

    #[test]
    fn absent_means_on_with_the_default_budget() {
        assert_eq!(load(None), Ok(Some(64 * MIB)));
        assert_eq!(load(Some("{}")), Ok(Some(64 * MIB)));
    }

    #[test]
    fn a_store_can_turn_it_off_or_size_it_within_bounds() {
        assert_eq!(load(Some(r#"{"mode":"off"}"#)), Ok(None));
        assert_eq!(
            load(Some(r#"{"mode":"on","max_bytes":2097152}"#)),
            Ok(Some(2 * MIB))
        );
        for bad in [
            r#"{"max_bytes":1}"#,
            r#"{"max_bytes":99999999999999}"#,
            r#"{"mode":"maybe"}"#,
            r#"{"size":1}"#,
            "not json",
        ] {
            assert!(load(Some(bad)).is_err(), "accepted {bad}");
        }
    }
}
