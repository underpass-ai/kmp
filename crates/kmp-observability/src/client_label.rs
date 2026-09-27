//! A host's self-declared name or version, reduced to what a log may carry:
//! printable ASCII from a small set, trimmed and bounded, so no caller can
//! put arbitrary text into a line.

/// Characters of a client name a log keeps.
pub const CLIENT_NAME_CHARS: usize = 64;
/// Characters of a client version a log keeps.
pub const CLIENT_VERSION_CHARS: usize = 32;

/// `text` with every character outside `[A-Za-z0-9 -_./@+:]` dropped,
/// trimmed and cut to `limit` characters.
pub fn client_label(text: &str, limit: usize) -> String {
    text.trim()
        .chars()
        .filter(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '@' | '+' | ' ' | ':')
        })
        .take(limit)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::client_label;

    #[test]
    fn keeps_a_name_and_drops_what_a_log_must_not_carry() {
        assert_eq!(client_label(" codex-mcp-client ", 64), "codex-mcp-client");
        assert_eq!(client_label("tonic/0.14 \"x\"\n", 64), "tonic/0.14 x");
        assert_eq!(client_label(&"y".repeat(100), 32).len(), 32);
        assert_eq!(client_label("\u{1F600}", 8), "");
    }
}
