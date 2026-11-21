//! The one spelling set every on/off switch accepts.
//!
//! Each switch used to have its own: `KMP_VERSION_CHECK` knew only a
//! lowercase `off`, `KMP_MCP_OUTPUT_SCHEMAS` only `0` and `1`,
//! `KMP_LEXICAL_INDEX=0` meant nothing and left the index on. Now every
//! boolean-like switch reads the same words, case-insensitively, and refuses
//! anything else.

/// What an on/off switch accepts, as a refusal names it.
pub const ON_OFF_ACCEPTED: &str = "on, off, true, false, 1, 0, yes, no (case does not matter)";

/// `Some(true)` for on, `Some(false)` for off, `None` for a word that is
/// neither. The value is trimmed first.
pub fn parse_on_off(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_on_off;

    #[test]
    fn every_switch_reads_the_same_words_in_any_case() {
        for on in ["on", "ON", " True ", "1", "yes", "Yes"] {
            assert_eq!(parse_on_off(on), Some(true), "{on}");
        }
        for off in ["off", "OFF", "false", "0", "no", " No "] {
            assert_eq!(parse_on_off(off), Some(false), "{off}");
        }
        for neither in ["", "offf", "enabled", "2", "y"] {
            assert_eq!(parse_on_off(neither), None, "{neither}");
        }
    }
}
