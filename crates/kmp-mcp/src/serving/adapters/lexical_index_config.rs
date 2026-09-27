use std::path::Path;

use serde::Deserialize;

use super::lexical_index::index_limits::IndexLimits;

/// The file beside a store that tunes when its lexical index is read.
pub(super) const LEXICAL_INDEX_CONFIG_FILE: &str = "lexical-index.json";

/// Per-store limits of the lexical index (P13):
/// `{"max_candidate_share_percent":35,"min_about_entries":2250}`, either key
/// optional. Absent, the store gets [`IndexLimits::DEFAULT`]; unreadable or
/// out of range, it is ignored and reported so, and the default applies.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LexicalIndexConfig {
    #[serde(default)]
    max_candidate_share_percent: Option<u8>,
    #[serde(default)]
    min_about_entries: Option<u64>,
}

impl LexicalIndexConfig {
    pub(super) fn load(data_dir: &Path) -> Result<IndexLimits, String> {
        let path = data_dir.join(LEXICAL_INDEX_CONFIG_FILE);
        if !path.is_file() {
            return Ok(IndexLimits::DEFAULT);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{LEXICAL_INDEX_CONFIG_FILE} is unreadable: {error}"))?;
        Self::parse(&text)
    }

    fn parse(text: &str) -> Result<IndexLimits, String> {
        let config: Self = serde_json::from_str(text).map_err(|error| {
            format!("{LEXICAL_INDEX_CONFIG_FILE} is not a lexical index configuration: {error}")
        })?;
        let defaults = IndexLimits::DEFAULT;
        let share = config
            .max_candidate_share_percent
            .unwrap_or(defaults.max_candidate_share_percent);
        if !(1..=100).contains(&share) {
            return Err(format!(
                "{LEXICAL_INDEX_CONFIG_FILE} max_candidate_share_percent is {share}; it must be 1 to 100"
            ));
        }
        Ok(IndexLimits {
            max_candidate_share_percent: share,
            min_about_entries: config
                .min_about_entries
                .unwrap_or(defaults.min_about_entries),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn either_limit_may_be_set_and_the_other_keeps_its_default() {
        assert_eq!(LexicalIndexConfig::parse("{}"), Ok(IndexLimits::DEFAULT));
        assert_eq!(
            LexicalIndexConfig::parse(r#"{"max_candidate_share_percent":50}"#),
            Ok(IndexLimits {
                max_candidate_share_percent: 50,
                ..IndexLimits::DEFAULT
            })
        );
        assert_eq!(
            LexicalIndexConfig::parse(r#"{"min_about_entries":0}"#),
            Ok(IndexLimits::DEFAULT.every_about())
        );
        assert!(LexicalIndexConfig::parse(r#"{"max_candidate_share_percent":0}"#).is_err());
        assert!(LexicalIndexConfig::parse(r#"{"max_candidate_share_percent":101}"#).is_err());
        assert!(LexicalIndexConfig::parse(r#"{"share":35}"#).is_err());
    }

    #[test]
    fn a_store_without_the_file_gets_the_defaults() {
        let directory = tempfile::tempdir().expect("store");
        assert_eq!(
            LexicalIndexConfig::load(directory.path()),
            Ok(IndexLimits::DEFAULT)
        );
        std::fs::write(
            directory.path().join(LEXICAL_INDEX_CONFIG_FILE),
            r#"{"min_about_entries":5}"#,
        )
        .expect("config");
        assert_eq!(
            LexicalIndexConfig::load(directory.path()).map(|limits| limits.min_about_entries),
            Ok(5)
        );
    }
}
