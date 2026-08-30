use std::fmt::Write as _;

use crate::banner;
use crate::lifecycle::application::use_cases::diagnose_tool_surface::diagnose_tool_surface;
use crate::lifecycle::domain::diagnosis_verdict::worst_severity;
use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::style::Style;

use super::agent_policy_probe::agent_policy_finding;
use super::backend_probe::backend_finding;
use super::diagnosis_render::{section, sgr};
use super::durability_probe::committed_bundle_finding;
use super::embedded_memory_probe::{compiled_formats, data_dir_finding};
use super::startup_log_probe::startup_history;
use super::telemetry_probe::telemetry_finding;
use super::viewer_probe::viewer_finding;

/// `doctor` — the same facts, judged, ending in the one thing to fix.
///
/// Returns the report and the exit code, so a script can gate on it.
pub fn doctor() -> (String, i32) {
    doctor_styled(Style::for_stdout())
}

fn lifecycle_findings() -> Vec<LifecycleFinding> {
    // The host diagnosis already speaks this report's vocabulary; there used
    // to be a second Level/Finding pair here and a hand-written conversion.
    crate::lifecycle::NativeLifecycle::diagnose()
        .findings()
        .to_vec()
}

pub(crate) fn doctor_styled(style: Style) -> (String, i32) {
    doctor_styled_with_lifecycle(style, lifecycle_findings())
}

pub(crate) fn doctor_styled_with_lifecycle(
    style: Style,
    lifecycle: Vec<LifecycleFinding>,
) -> (String, i32) {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}\n",
        banner::large_with(style, "  doctor — agent memory, end to end")
    );

    let binary = LifecycleFinding::new(
        DiagnosticSeverity::Ok,
        format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
    )
    .with_detail(format!("store formats: {}", compiled_formats()));
    section(&mut out, style, "Binary", &[binary]);

    let backend = backend_finding();
    section(&mut out, style, "Backend", std::slice::from_ref(&backend));

    let (data_dir, resolved) = data_dir_finding();
    let data_dir_level = data_dir.severity();
    section(&mut out, style, "Memory", &[data_dir]);
    let durability = resolved.as_ref().and_then(committed_bundle_finding);
    let durability_level = durability
        .as_ref()
        .map_or(DiagnosticSeverity::Ok, |finding| finding.severity());
    if let Some(durability) = durability {
        section(&mut out, style, "Durability", &[durability]);
    }

    let tools = crate::tool_names();
    let surface = diagnose_tool_surface(&tools, &crate::contract::declared_tool_names());
    let surface_level = surface.severity();
    section(&mut out, style, "Tools", &[surface]);
    let lifecycle_level = worst_severity(lifecycle.iter().map(|finding| finding.severity()));
    section(&mut out, style, "Hosts", &lifecycle);
    let agent_policy = agent_policy_finding();
    let agent_policy_level = agent_policy.severity();
    section(&mut out, style, "Agent", &[agent_policy]);
    section(&mut out, style, "Viewer", &[viewer_finding()]);
    let telemetry = resolved.as_ref().map(telemetry_finding);
    let telemetry_level = telemetry
        .as_ref()
        .map_or(DiagnosticSeverity::Ok, |finding| finding.severity());
    if let Some(telemetry) = telemetry {
        section(&mut out, style, "Telemetry", &[telemetry]);
    }

    let mut history_level = DiagnosticSeverity::Ok;
    if let Some(resolved) = resolved.as_ref() {
        let history = startup_history(resolved.path(), 5);
        let finding = if history.is_empty() {
            history_level = DiagnosticSeverity::Warn;
            LifecycleFinding::new(
                DiagnosticSeverity::Warn,
                "this memory has never been started here",
            )
            .with_detail("a host that never started leaves no line to read")
        } else {
            let mut recent = LifecycleFinding::new(DiagnosticSeverity::Ok, "recent startups");
            for line in history {
                recent = recent.with_detail(line);
            }
            recent
        };
        section(&mut out, style, "History", &[finding]);
    }

    let worst = [
        data_dir_level,
        durability_level,
        surface_level,
        lifecycle_level,
        backend.severity(),
        history_level,
        agent_policy_level,
        telemetry_level,
    ]
    .into_iter();
    let worst = worst_severity(worst);

    // The verdict wears the worst finding's ink: the one line a reader
    // scrolls to is the one line that should be findable at a glance.
    match worst {
        DiagnosticSeverity::Fail => {
            let _ = writeln!(
                out,
                "{}",
                style.paint(
                    sgr(DiagnosticSeverity::Fail),
                    "Not usable. Fix the FAIL above first."
                )
            );
            (out, 1)
        }
        DiagnosticSeverity::Warn => {
            let _ = writeln!(
                out,
                "{}",
                style.paint(
                    sgr(DiagnosticSeverity::Warn),
                    "Usable, with a warning above."
                )
            );
            (out, 0)
        }
        DiagnosticSeverity::Ok => {
            let _ = writeln!(
                out,
                "{}",
                style.paint(sgr(DiagnosticSeverity::Ok), "Usable.")
            );
            (out, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::doctor_styled_with_lifecycle;
    use crate::banner;
    use crate::lifecycle::adapters::info_report::{info, info_styled};
    use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
    use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
    use crate::style::Style;

    fn lifecycle_fixture() -> Vec<LifecycleFinding> {
        vec![LifecycleFinding::new(
            DiagnosticSeverity::Ok,
            "native hosts use the tested lifecycle",
        )]
    }

    /// The mark reaches a user through `/kmp:info` and `/kmp:doctor` and
    /// nowhere else — the startup banner goes to stderr and the host eats it,
    /// and nobody runs `--help` on a server a plugin launches. So a branded
    /// surface that quietly stops being branded should fail here rather than
    /// be noticed by nobody, which is what happened to the mark that was
    /// written, tested and never rendered.
    #[test]
    fn the_two_surfaces_a_user_actually_reaches_carry_the_mark() {
        let (doctor_report, _) = doctor_styled_with_lifecycle(Style::Plain, lifecycle_fixture());
        for (surface, report) in [("info", info()), ("doctor", doctor_report)] {
            assert!(
                report.starts_with(banner::LARGE),
                "`{surface}` must open with the mark"
            );
            assert!(
                report.contains("Kernel Memory Protocol"),
                "`{surface}` must say what KMP is"
            );
        }
    }
    /// A terminal gets ink; a pipe gets the pinned bytes. Both must say the
    /// same thing, or the human and the plugin host are reading different
    /// products.
    #[test]
    fn styled_reports_say_exactly_what_plain_reports_say() {
        assert_eq!(
            crate::style::stripped(&info_styled(Style::Ansi)),
            info_styled(Style::Plain)
        );
        let (styled, _) = doctor_styled_with_lifecycle(Style::Ansi, lifecycle_fixture());
        let (plain, _) = doctor_styled_with_lifecycle(Style::Plain, lifecycle_fixture());
        assert_eq!(crate::style::stripped(&styled), plain);
    }
    /// A failing host finding must reach the verdict and the exit code —
    /// this is the line a script gates on, and a mutation probe showed
    /// nothing else pinned it.
    #[test]
    fn a_failing_finding_makes_the_verdict_say_not_usable_and_exit_nonzero() {
        let failing = vec![LifecycleFinding::new(
            DiagnosticSeverity::Fail,
            "a host is broken on purpose",
        )];
        let (report, code) = doctor_styled_with_lifecycle(Style::Plain, failing);
        assert_eq!(code, 1);
        let verdict = report.lines().rfind(|line| !line.trim().is_empty());
        assert_eq!(
            verdict,
            Some("Not usable. Fix the FAIL above first."),
            "the verdict wears the failure"
        );
    }

    #[test]
    fn doctor_ends_in_a_verdict() {
        let (report, code) = doctor_styled_with_lifecycle(Style::Plain, lifecycle_fixture());
        assert!(report.contains("▌KMP▐ Binary"));
        assert!(report.contains("▌KMP▐ Tools"));
        let verdict = report.lines().rfind(|line| !line.trim().is_empty());
        assert!(
            verdict
                .is_some_and(|line| line.starts_with("Usable") || line.starts_with("Not usable")),
            "the last word is a verdict: {verdict:?}"
        );
        assert!(code == 0 || code == 1);
    }
}
