//! What KMP shows when it announces itself.
//!
//! One mark and one lockup. The mark says what this binary is to someone
//! meeting it, and heads `help`, `info`, `doctor` and a startup. The lockup —
//! `▌KMP▐ Backend ────` — says which tool is talking at the head of a section,
//! in one line, so the brand never pushes the answer off the screen.
//!
//! There used to be a third: a four-line compact logo this doc described as
//! heading those sections. Nothing outside this file ever called it. Branded
//! code that never renders is worse than none — it reads as done in review and
//! is absent in use — so it went, and the doc now describes what exists.

/// The full mark, with what KMP is and what it does not need.
///
/// Written without a `\` line continuation on purpose: that escape eats the
/// newline *and* the leading whitespace of the next line, so the top row of
/// the mark lost its column and the logo shipped one character out of true —
/// on the two surfaces a user actually meets it.
pub const LARGE: &str = " ██╗  ██╗███╗   ███╗██████╗
 ██║ ██╔╝████╗ ████║██╔══██╗   Kernel Memory Protocol
 █████╔╝ ██╔████╔██║██████╔╝   temporal · multidimensional · auditable
 ██╔═██╗ ██║╚██╔╝██║██╔═══╝
 ██║  ██╗██║ ╚═╝ ██║██║        embedded database + event store
 ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝        no external services";

/// The large mark with a subtitle under it.
pub fn large_with(subtitle: &str) -> String {
    format!("{LARGE}\n\n{subtitle}")
}

/// A section head: one line, so the mark never pushes the answer off screen.
pub fn head(title: &str) -> String {
    let lockup = format!("▌KMP▐ {title} ");
    let width = lockup.chars().count();
    let rule = "─".repeat(WIDTH.saturating_sub(width));
    format!("{lockup}{rule}")
}

/// How wide a section head runs, chosen to fit an 80-column terminal.
const WIDTH: usize = 72;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_large_mark_says_what_kmp_is_and_what_it_does_not_need() {
        assert!(LARGE.contains("Kernel Memory Protocol"));
        assert!(LARGE.contains("embedded database + event store"));
        assert!(LARGE.contains("no external services"));
    }

    /// The mark is the product's face on the only two surfaces a user
    /// reaches. A row one column out of true is the kind of thing nobody
    /// reports and everybody sees.
    #[test]
    fn every_row_of_the_mark_starts_in_the_same_column() {
        let indents = LARGE
            .lines()
            .map(|line| line.len() - line.trim_start().len())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            indents.len(),
            1,
            "rows start at different columns: {indents:?}"
        );
        assert_eq!(LARGE.lines().count(), 6);
    }

    #[test]
    fn a_section_head_is_one_line_and_names_its_section() {
        let head = head("Backend");
        assert_eq!(head.lines().count(), 1);
        assert!(head.contains("KMP"));
        assert!(head.contains("Backend"));
        assert_eq!(head.chars().count(), WIDTH, "heads line up with each other");
    }

    #[test]
    fn a_long_title_does_not_wrap_the_head() {
        let head = head("A section with a very long name that eats the whole rule");
        assert_eq!(head.lines().count(), 1);
    }
}
