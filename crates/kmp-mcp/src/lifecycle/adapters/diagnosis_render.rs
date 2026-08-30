use std::fmt::Write as _;

use crate::banner;
use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::style::{self, Style};

/// The severity's column tag. The words already carry the verdict; the tag
/// makes the column scannable.
pub(crate) fn tag(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Ok => "ok  ",
        DiagnosticSeverity::Warn => "warn",
        DiagnosticSeverity::Fail => "FAIL",
    }
}

/// The tag's ink. On a terminal the color lets a reader find the one line
/// that matters without reading.
pub(crate) fn sgr(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Ok => style::OK,
        DiagnosticSeverity::Warn => style::WARN,
        DiagnosticSeverity::Fail => style::FAIL,
    }
}

/// Wraps a detail line so a terminal never has to. Long guidance is the point
/// of a diagnostic; wrapping it by hand is how it stops being read.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

pub(crate) fn section(out: &mut String, style: Style, title: &str, findings: &[LifecycleFinding]) {
    let _ = writeln!(out, "{}", banner::head_styled(style, title));
    for finding in findings {
        let _ = writeln!(
            out,
            "  {}  {}",
            style.paint(sgr(finding.severity()), tag(finding.severity())),
            finding.headline()
        );
        for line in finding.detail() {
            for wrapped in wrap(line, 62) {
                let _ = writeln!(out, "        {wrapped}");
            }
        }
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn a_detail_line_wraps_instead_of_asking_the_terminal_to() {
        let long = "the binary is fail-fast: with no configuration it exits with guidance \
                    rather than guessing, so a host usually sets this in its registration";
        let wrapped = wrap(long, 40);
        assert!(wrapped.len() > 1);
        assert!(wrapped.iter().all(|line| line.chars().count() <= 40));
        assert_eq!(
            wrapped.join(" ").split_whitespace().count(),
            long.split_whitespace().count()
        );
    }
    #[test]
    fn a_short_line_is_left_alone() {
        assert_eq!(
            wrap("store format: 2", 40),
            vec!["store format: 2".to_string()]
        );
        assert!(wrap("", 40).is_empty());
    }
}
