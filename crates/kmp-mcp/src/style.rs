//! Whether output wears color, decided once per stream.
//!
//! Color here is an envelope, never a different string: everything styled
//! degrades to the exact plain bytes the tests pin. That matters because the
//! two surfaces most people meet (`/kmp:info`, `/kmp:doctor`) read stdout
//! through a pipe — a plugin host must keep getting the bytes it always got,
//! and ink appears only when a human is looking at a terminal.

use std::io::IsTerminal;

/// Green for fine, yellow for worth-a-look.
pub const OK: &str = "32";
pub const WARN: &str = "33";
/// Red and bold: a failure reads as a failure, always.
pub const FAIL: &str = "1;31";
/// For rules and furniture — present, but never competing with the answer.
pub const DIM: &str = "2";

/// Plain or ANSI, resolved from the stream the text is about to hit.
///
/// A `String`-returning renderer cannot ask "am I a terminal?" at print
/// time, so the caller decides where the stream is known and threads the
/// answer through. `NO_COLOR` (<https://no-color.org>) and `TERM=dumb` both
/// mean plain even on a terminal: both are the user saying no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    Ansi,
}

impl Style {
    pub fn for_stdout() -> Self {
        Self::for_terminal(std::io::stdout().is_terminal())
    }

    pub fn for_stderr() -> Self {
        Self::for_terminal(std::io::stderr().is_terminal())
    }

    fn for_terminal(is_terminal: bool) -> Self {
        let refused = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
            || std::env::var_os("TERM").is_some_and(|term| term == "dumb");
        if is_terminal && !refused {
            Style::Ansi
        } else {
            Style::Plain
        }
    }

    /// Wraps `text` in an SGR sequence — or does not, which is the point.
    pub fn paint(self, sgr: &str, text: &str) -> String {
        match self {
            Style::Plain => text.to_string(),
            Style::Ansi => format!("\x1b[{sgr}m{text}\x1b[0m"),
        }
    }

    /// Truecolor foreground, for the mark's gradient. Every terminal this
    /// project meets in practice speaks it, and the ones that do not are the
    /// pipes and dumb terminals that already resolved to `Plain`.
    pub fn rgb(self, (r, g, b): (u8, u8, u8), text: &str) -> String {
        match self {
            Style::Plain => text.to_string(),
            Style::Ansi => format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m"),
        }
    }
}

/// The styled text with every SGR sequence removed. Tests use it to prove
/// that ink never changes the words underneath.
#[cfg(test)]
pub fn stripped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_is_a_passthrough_byte_for_byte() {
        assert_eq!(Style::Plain.paint(OK, "fine"), "fine");
        assert_eq!(Style::Plain.rgb((1, 2, 3), "fine"), "fine");
    }

    #[test]
    fn ansi_wraps_and_always_resets() {
        let painted = Style::Ansi.paint(FAIL, "broken");
        assert!(painted.starts_with("\x1b[1;31m"));
        assert!(painted.ends_with("\x1b[0m"));
        assert_eq!(stripped(&painted), "broken");
    }

    #[test]
    fn a_pipe_never_wears_color() {
        assert_eq!(Style::for_terminal(false), Style::Plain);
    }

    #[test]
    fn stripping_undoes_any_mix_of_ink_and_text() {
        let mixed = format!(
            "{} and {}",
            Style::Ansi.rgb((7, 7, 7), "this"),
            Style::Ansi.paint(DIM, "that")
        );
        assert_eq!(stripped(&mixed), "this and that");
    }
}
