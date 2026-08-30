use std::fmt::Write as _;

use crate::banner;
use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::style::Style;

use super::agent_policy_probe::agent_policy_finding;
use super::backend_probe::backend_finding;
use super::diagnosis_render::section;
use super::durability_probe::committed_bundle_finding;
use super::embedded_memory_probe::{compiled_formats, data_dir_finding};
use super::machine_memories_probe::memories_finding;
use super::startup_log_probe::startup_history;
use super::telemetry_probe::telemetry_finding;
use super::viewer_probe::viewer_finding;

/// `info` — the facts, with no verdict: what this binary is and what memory it
/// would open here. Styled for whatever stdout is: a pipe gets the pinned
/// plain bytes, a terminal gets ink.
pub fn info() -> String {
    info_styled(Style::for_stdout())
}

pub(crate) fn info_styled(style: Style) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}\n",
        banner::large_with(
            style,
            &format!(
                "  {} {}   ·   store formats {}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
                compiled_formats()
            )
        )
    );

    section(&mut out, style, "Backend", &[backend_finding()]);
    let (data_dir, resolved) = data_dir_finding();
    section(&mut out, style, "Memory", &[data_dir]);
    if let Some(durability) = resolved.as_ref().and_then(committed_bundle_finding) {
        section(&mut out, style, "Durability", &[durability]);
    }

    let names = crate::tool_names();
    let surface = LifecycleFinding::new(
        DiagnosticSeverity::Ok,
        format!("{} tools on the MCP surface", names.len()),
    )
    .with_detail(names.join(" "));
    section(&mut out, style, "Tools", &[surface]);
    section(&mut out, style, "Agent", &[agent_policy_finding()]);
    section(&mut out, style, "Viewer", &[viewer_finding()]);
    if let Some(resolved) = resolved.as_ref() {
        section(&mut out, style, "Telemetry", &[telemetry_finding(resolved)]);
    }
    section(&mut out, style, "Memories", &memories_finding());

    if let Some(resolved) = resolved {
        let history = startup_history(resolved.path(), 3);
        if !history.is_empty() {
            let mut recent = LifecycleFinding::new(DiagnosticSeverity::Ok, "recent startups");
            for line in history {
                recent = recent.with_detail(line);
            }
            section(&mut out, style, "History", &[recent]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::info;

    #[test]
    fn info_reports_the_surface_without_judging_it() {
        let report = info();
        assert!(report.contains("Kernel Memory Protocol"));
        assert!(report.contains("13 tools on the MCP surface"));
        assert!(report.contains("kmp_write_memory"));
        assert!(!report.contains("Usable"), "info states, doctor judges");
    }
}
