use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;

/// Where a human can watch this memory. The capability belongs to the running
/// session, so this separate diagnostic never prints a bare URL that will 401.
pub(crate) fn viewer_finding() -> LifecycleFinding {
    match crate::viewer::viewer_addr_from_env().addr() {
        Some(_) => LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            "ChronoLoom comes with an embedded session",
        )
        .with_detail("ask the agent to open it — only that session knows its capability link"),
        None => LifecycleFinding::new(
            DiagnosticSeverity::Warn,
            "declined — no viewer this session",
        )
        .with_detail(format!(
            "unset {} and restart the session to see your memory again",
            kmp_viewer::VIEWER_ADDR_ENV
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::viewer_finding;

    #[test]
    fn diagnostics_never_offer_an_unauthorised_viewer_url() {
        let finding = viewer_finding();
        assert!(
            !finding.headline().contains("http://")
                && finding
                    .detail()
                    .iter()
                    .all(|line| !line.contains("http://")),
            "a separate process cannot know the running session's capability: {finding:?}"
        );
    }
}
