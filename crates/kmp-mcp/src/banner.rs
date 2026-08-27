//! What KMP shows when it announces itself.
//!
//! One mark and one lockup. The mark says what this binary is to someone
//! meeting it, and heads `help`, `info`, `doctor` and a startup. The lockup —
//! `▌KMP▐ Backend ────` — says which tool is talking at the head of a section,
//! in one line, so the brand never pushes the answer off the screen.
//!
//! On a terminal the letterforms wear a violet-to-green gradient anchored to
//! the viewer's palette, so the CLI and the browser read as one product. On a
//! pipe — which is what a plugin host is — everything degrades to `LARGE`
//! byte for byte: the color is an envelope, never a different string.
//!
//! There used to be a third: a four-line compact logo this doc described as
//! heading those sections. Nothing outside this file ever called it. Branded
//! code that never renders is worse than none — it reads as done in review and
//! is absent in use — so it went, and the doc now describes what exists.

use crate::style::{self, Style};

/// The full mark, with what KMP is and what it does not need.
///
/// Written without a `\` line continuation on purpose: that escape eats the
/// newline *and* the leading whitespace of the next line, so the top row of
/// the mark lost its column and the logo shipped one character out of true —
/// on the two surfaces a user actually meets it.
pub const LARGE: &str = " ██╗  ██╗███╗   ███╗██████╗
 ██║ ██╔╝████╗ ████║██╔══██╗   Kernel Memory Protocol
 █████╔╝ ██╔████╔██║██████╔╝   time travel over a graph, proofs attached
 ██╔═██╗ ██║╚██╔╝██║██╔═══╝    temporal · multidimensional · auditable
 ██║  ██╗██║ ╚═╝ ██║██║        embedded database + event store
 ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝        no external services";

/// One color per row of the mark, violet sweeping to green through the
/// viewer's accent blue — the same three anchors the graph's nodes wear, so
/// the terminal and the browser are recognizably the same product.
const GRADIENT: [(u8, u8, u8); 6] = [
    (129, 91, 240),
    (100, 106, 233),
    (72, 120, 224),
    (52, 141, 192),
    (38, 159, 155),
    (27, 175, 122),
];

/// The ink the lockup and the pulse share: the middle of the gradient,
/// which is the accent the viewer already wears.
pub const ACCENT: (u8, u8, u8) = GRADIENT[2];

/// The block-drawing glyphs are the letterforms; everything else on a row is
/// words. Only the letterforms take the gradient — a colored tagline would
/// make the reader parse ink to find the facts.
fn is_letterform(glyph: char) -> bool {
    matches!(glyph, '█' | '╔' | '╗' | '╚' | '╝' | '║' | '═')
}

/// The mark, wearing the stream's style. `Style::Plain` is `LARGE` exactly.
pub fn large(style: Style) -> String {
    if style == Style::Plain {
        return LARGE.to_string();
    }
    LARGE
        .lines()
        .enumerate()
        .map(|(row, line)| {
            let ink = GRADIENT[row.min(GRADIENT.len() - 1)];
            let mut out = String::new();
            let mut run = String::new();
            for glyph in line.chars() {
                if is_letterform(glyph) {
                    run.push(glyph);
                } else {
                    if !run.is_empty() {
                        out.push_str(&style.rgb(ink, &run));
                        run.clear();
                    }
                    out.push(glyph);
                }
            }
            if !run.is_empty() {
                out.push_str(&style.rgb(ink, &run));
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The mark with a subtitle under it.
pub fn large_with(style: Style, subtitle: &str) -> String {
    format!("{}\n\n{subtitle}", large(style))
}

/// A section head: one line, so the mark never pushes the answer off screen.
/// The lockup takes the accent, the rule dims, and the title stays bare —
/// it is the thing being read.
pub fn head_styled(style: Style, title: &str) -> String {
    let lockup = format!("▌KMP▐ {title} ");
    let width = lockup.chars().count();
    let rule = "─".repeat(WIDTH.saturating_sub(width));
    if style == Style::Plain {
        return format!("{lockup}{rule}");
    }
    let (r, g, b) = ACCENT;
    format!(
        "{} {title} {}",
        style.paint(&format!("1;38;2;{r};{g};{b}"), "▌KMP▐"),
        style.paint(style::DIM, &rule)
    )
}

/// The plain head, for callers that already know they are on a pipe.
pub fn head(title: &str) -> String {
    head_styled(Style::Plain, title)
}

/// How wide a section head runs, chosen to fit an 80-column terminal.
const WIDTH: usize = 72;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::stripped;

    #[test]
    fn the_large_mark_says_what_kmp_is_and_what_it_does_not_need() {
        assert!(LARGE.contains("Kernel Memory Protocol"));
        assert!(LARGE.contains("time travel over a graph, proofs attached"));
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

    /// The taglines form a second column, and that column is only a column
    /// if every line of it starts in the same place.
    #[test]
    fn every_tagline_starts_in_the_same_column() {
        let starts = LARGE
            .lines()
            .filter_map(|line| line.chars().position(|glyph| glyph.is_alphabetic()))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            starts.len(),
            1,
            "taglines start at different columns: {starts:?}"
        );
    }

    /// A host reads the mark through a pipe; a human reads it on a terminal.
    /// Same words, always — the gradient must never change what is said.
    #[test]
    fn the_styled_mark_is_the_plain_mark_underneath() {
        assert_eq!(large(Style::Plain), LARGE);
        assert_eq!(stripped(&large(Style::Ansi)), LARGE);
        assert_ne!(large(Style::Ansi), LARGE, "a terminal actually gets ink");
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
    fn a_styled_head_lines_up_with_a_plain_one() {
        let styled = head_styled(Style::Ansi, "Backend");
        assert_eq!(stripped(&styled), head("Backend"));
    }

    #[test]
    fn a_long_title_does_not_wrap_the_head() {
        let head = head("A section with a very long name that eats the whole rule");
        assert_eq!(head.lines().count(), 1);
    }
}
