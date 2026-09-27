//! A store's lazy-page sizes (P14): read the same way by every transport.

use serde_json::Value;

use super::rank_depth::{RANK_DEPTH_CHUNK, RANK_HEAD_WINDOW};

/// The file beside a store that holds them, with the lexical index limits.
pub const RANK_PAGES_FILE: &str = "lexical-index.json";

/// How many eligible candidates make an ask's head, and how many tail items
/// a `kmp2` continuation reads past its offset: `head_window` and
/// `continuation_chunk` in [`RANK_PAGES_FILE`], 64 each by default, 1 to 4096.
/// MCP and the gRPC server read them from the same file with this one rule,
/// so both answer an ask of the store with the same pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankPages {
    pub head_window: usize,
    pub continuation_chunk: usize,
}

impl Default for RankPages {
    fn default() -> Self {
        Self {
            head_window: RANK_HEAD_WINDOW,
            continuation_chunk: RANK_DEPTH_CHUNK,
        }
    }
}

impl RankPages {
    /// Both sizes, each in range, or why not.
    pub fn new(head_window: usize, continuation_chunk: usize) -> Result<Self, String> {
        for (key, value) in [
            ("head_window", head_window),
            ("continuation_chunk", continuation_chunk),
        ] {
            if !(1..=4096).contains(&value) {
                return Err(format!(
                    "{RANK_PAGES_FILE} {key} is {value}; it must be 1 to 4096"
                ));
            }
        }
        Ok(Self {
            head_window,
            continuation_chunk,
        })
    }

    /// The sizes the file's text names, the default for a key it omits. Other
    /// keys (the index limits) are the reader's own business.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let value = serde_json::from_str::<Value>(text)
            .map_err(|error| format!("{RANK_PAGES_FILE} is not JSON: {error}"))?;
        let defaults = Self::default();
        let read = |key: &str, default: usize| match value.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(found) => found
                .as_u64()
                .and_then(|found| usize::try_from(found).ok())
                .ok_or_else(|| format!("{RANK_PAGES_FILE} {key} must be an integer")),
        };
        Self::new(
            read("head_window", defaults.head_window)?,
            read("continuation_chunk", defaults.continuation_chunk)?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sizes_are_read_with_their_defaults_and_range() {
        assert_eq!(RankPages::from_json("{}"), Ok(RankPages::default()));
        assert_eq!(
            RankPages::from_json(r#"{"head_window":16,"min_about_entries":0}"#),
            Ok(RankPages {
                head_window: 16,
                continuation_chunk: 64
            })
        );
        assert!(RankPages::from_json(r#"{"continuation_chunk":0}"#).is_err());
        assert!(RankPages::from_json(r#"{"head_window":"x"}"#).is_err());
    }
}
