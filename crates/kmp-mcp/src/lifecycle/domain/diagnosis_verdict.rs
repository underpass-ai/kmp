use super::diagnostic_severity::DiagnosticSeverity;

/// The worst finding decides the verdict: the one line a reader scrolls to
/// must reflect the one problem that matters.
pub fn worst_severity(
    severities: impl IntoIterator<Item = DiagnosticSeverity>,
) -> DiagnosticSeverity {
    severities
        .into_iter()
        .max_by_key(|severity| match severity {
            DiagnosticSeverity::Ok => 0,
            DiagnosticSeverity::Warn => 1,
            DiagnosticSeverity::Fail => 2,
        })
        .unwrap_or(DiagnosticSeverity::Ok)
}

#[cfg(test)]
mod tests {
    use super::worst_severity;
    use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;

    #[test]
    fn the_worst_severity_wins_and_silence_is_ok() {
        use DiagnosticSeverity::{Fail, Ok, Warn};
        assert_eq!(worst_severity([Ok, Warn, Ok]), Warn);
        assert_eq!(worst_severity([Warn, Fail, Ok]), Fail);
        assert_eq!(worst_severity([]), Ok);
    }
}
