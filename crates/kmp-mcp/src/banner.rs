//! What KMP shows when it announces itself.
//!
//! Two marks, because they answer different questions. The large one says what
//! this binary is to someone meeting it — it heads `help` and a startup. The
//! compact one only has to say *which* tool is talking, so it heads a section
//! inside `info` and `doctor` without pushing the answer off the screen.

/// The full mark, with what KMP is and what it does not need.
pub const LARGE: &str = "\
 ██╗  ██╗███╗   ███╗██████╗
 ██║ ██╔╝████╗ ████║██╔══██╗   Kernel Memory Protocol
 █████╔╝ ██╔████╔██║██████╔╝   temporal · multidimensional · auditable
 ██╔═██╗ ██║╚██╔╝██║██╔═══╝
 ██║  ██╗██║ ╚═╝ ██║██║        embedded database + event store
 ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝        no external services";

/// The compact mark, for the head of a section.
pub const SMALL: &str = "\
▗▖ ▗▖▗▖  ▗▖▗▄▄▖
▐▌▗▞▘▐▛▚▞▜▌▐▌ ▐▌
▐▛▚▖ ▐▌  ▐▌▐▛▀▘
▐▌ ▐▌▐▌  ▐▌▐▌";

/// The large mark with a subtitle under it.
pub fn large_with(subtitle: &str) -> String {
    format!("{LARGE}\n\n{subtitle}")
}

/// The compact mark beside a title, for a section head.
pub fn small_with(title: &str) -> String {
    let mut out = String::new();
    for (index, line) in SMALL.lines().enumerate() {
        if index == 1 {
            out.push_str(&format!("{line}   {title}\n"));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
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

    #[test]
    fn the_compact_mark_fits_a_section_head() {
        // Four lines is the budget: a section head must not push the answer
        // it introduces off a small terminal.
        assert_eq!(SMALL.lines().count(), 4);
        assert!(SMALL.lines().all(|line| line.chars().count() <= 20));
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

    #[test]
    fn a_section_head_carries_its_title_on_the_reading_line() {
        let head = small_with("Store");
        let titled: Vec<&str> = head.lines().filter(|line| line.contains("Store")).collect();
        assert_eq!(titled.len(), 1, "exactly one line carries the title");
        assert!(head.lines().count() == 4);
    }
}
