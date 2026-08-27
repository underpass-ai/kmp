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
/// `CLICOLOR_FORCE` (<https://bixense.com/clicolors/>) means ink even on a
/// pipe — for `less -R`, a CI log, a host that renders ANSI — and only an
/// explicit `NO_COLOR` outranks it: when the user says both, refusal is the
/// safer word to honor.
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
        Self::resolve(
            is_terminal,
            std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()),
            std::env::var_os("CLICOLOR_FORCE")
                .is_some_and(|value| !value.is_empty() && value != "0"),
            std::env::var_os("TERM").is_some_and(|term| term == "dumb"),
        )
    }

    /// The decision itself, separated from the environment so tests can hit
    /// every branch without mutating process-global state under a parallel
    /// test runner.
    fn resolve(is_terminal: bool, refused: bool, forced: bool, dumb: bool) -> Self {
        if refused {
            return Style::Plain;
        }
        if forced {
            return Style::Ansi;
        }
        if is_terminal && !dumb {
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
        assert_eq!(Style::resolve(false, false, false, false), Style::Plain);
    }

    #[test]
    fn forcing_inks_a_pipe_and_even_a_dumb_terminal() {
        assert_eq!(Style::resolve(false, false, true, false), Style::Ansi);
        assert_eq!(Style::resolve(true, false, true, true), Style::Ansi);
    }

    #[test]
    fn refusal_outranks_forcing() {
        assert_eq!(Style::resolve(true, true, true, false), Style::Plain);
    }

    #[test]
    fn a_dumb_terminal_stays_plain_unless_forced() {
        assert_eq!(Style::resolve(true, false, false, true), Style::Plain);
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
