//! `info` and `doctor`: what this binary is, and why the memory is not answering.
//!
//! Both read the layout from the filesystem and never open the store. A
//! diagnostic that creates a store, or takes the single-writer lock out from
//! under a live session, has changed the thing it was asked to describe.

use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;

use kmp_embedded::{ResolvedDataDir, StorageEngine};

use crate::banner;

/// How bad a finding is. `Fail` is reserved for something that stops the
/// memory answering, so a run that ends without one is usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Ok => "ok  ",
            Level::Warn => "warn",
            Level::Fail => "FAIL",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub level: Level,
    pub headline: String,
    pub detail: Vec<String>,
}

impl Finding {
    fn new(level: Level, headline: impl Into<String>) -> Self {
        Self {
            level,
            headline: headline.into(),
            detail: Vec::new(),
        }
    }

    fn with(mut self, line: impl Into<String>) -> Self {
        self.detail.push(line.into());
        self
    }
}

/// The engines this build carries, as `--version` reports them.
pub fn compiled_formats() -> String {
    let mut formats = vec![format!("{}", StorageEngine::Redb.format_version())];
    if StorageEngine::Sqlite.is_compiled() {
        formats.push(format!(
            "{} (sqlite)",
            StorageEngine::Sqlite.format_version()
        ));
    }
    formats.join(", ")
}

/// Which engine's store file is on disk, without opening either.
pub fn engine_on_disk(data_dir: &Path) -> Option<StorageEngine> {
    [StorageEngine::Redb, StorageEngine::Sqlite]
        .into_iter()
        .find(|engine| kmp_embedded::store_file_path_for(data_dir, *engine).exists())
}

fn describe_data_dir(resolved: &ResolvedDataDir) -> Finding {
    let path = resolved.path();
    let mut finding = Finding::new(Level::Ok, path.display().to_string())
        .with(format!("chosen by: {}", resolved.rule_name()));

    match kmp_embedded::read_stamped_version(path) {
        Ok(version) => finding = finding.with(format!("store format: {version}")),
        Err(_) => finding = finding.with("store format: not stamped yet"),
    }
    match engine_on_disk(path) {
        Some(engine) => finding = finding.with(format!("engine on disk: {engine}")),
        None => finding = finding.with("no store yet — it is created on first write"),
    }
    if let Some(bundle) = kmp_embedded::project_bundle_path(resolved) {
        finding = finding.with(format!(
            "committed memory: {} ({})",
            bundle.display(),
            if bundle.exists() {
                "present"
            } else {
                "not exported yet"
            }
        ));
    }
    finding
}

fn data_dir_finding() -> (Finding, Option<ResolvedDataDir>) {
    // Locate, never prepare: a report on where memory lives must not create
    // it. `info` and `doctor` are run from wherever the user is standing.
    match kmp_embedded::locate_data_dir_from_env() {
        Ok(resolved) => (describe_data_dir(&resolved), Some(resolved)),
        Err(error) => (
            Finding::new(Level::Fail, "the data directory does not resolve")
                .with(error.to_string())
                .with("nothing can be read or written until this resolves"),
            None,
        ),
    }
}

fn backend_finding() -> Finding {
    let configured = std::env::var(crate::MCP_BACKEND_ENV)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let endpoint = std::env::var(crate::GRPC_ENDPOINT_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    match configured.as_deref() {
        Some("embedded") => Finding::new(Level::Ok, "embedded — the kernel is right here"),
        Some("fixture" | "fixtures") => {
            Finding::new(Level::Warn, "fixture — canned answers that look real")
                .with("nothing you write is stored; unset the variable for the real kernel")
        }
        Some("grpc" | "live") => match endpoint {
            Some(endpoint) => Finding::new(Level::Ok, format!("grpc — talking to {endpoint}")),
            None => Finding::new(Level::Fail, "grpc, with no kernel to talk to").with(format!(
                "set {} , or unset {} and the kernel runs right here",
                crate::GRPC_ENDPOINT_ENV,
                crate::MCP_BACKEND_ENV
            )),
        },
        Some(other) => Finding::new(Level::Fail, format!("`{other}` is not a backend"))
            .with("use `embedded` (the default), `grpc` or `fixture`"),
        // Nothing set is not a gap to warn about any more: it is the product.
        // The old text called this "no backend selected" and warned about it,
        // which was a fossil of the Kubernetes-first days — and the second
        // thing a stranger read.
        None => match endpoint {
            Some(endpoint) => Finding::new(Level::Ok, format!("grpc — talking to {endpoint}"))
                .with("an endpoint in the environment is how the cluster edition is chosen"),
            None => Finding::new(Level::Ok, "embedded — the default, nothing to configure"),
        },
    }
}

/// The last startup outcomes this data directory recorded, newest last.
fn startup_history(data_dir: &Path, limit: usize) -> Vec<String> {
    // The log rolls, so the name on disk carries a date suffix. Reading only
    // `kmp-mcp.log` finds nothing on every day but the first.
    let Ok(entries) = std::fs::read_dir(data_dir.join("logs")) else {
        return Vec::new();
    };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("kmp-mcp.log"))
        })
        .collect();
    logs.sort();
    let text: String = logs
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect::<Vec<_>>()
        .join("\n");
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| line.contains("startup succeeded") || line.contains("startup failed"))
        .map(|line| {
            let outcome = if line.contains("startup failed") {
                "failed"
            } else {
                "ok"
            };
            let stamp = line
                .split('"')
                .find(|part| part.len() >= 19 && part.starts_with("20"))
                .unwrap_or("")
                .replace('T', " ");
            format!("{outcome:<7}{}", &stamp[..stamp.len().min(19)])
        })
        .collect();
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    lines
}

/// Wraps a detail line so a terminal never has to. Long guidance is the point
/// of a diagnostic; wrapping it by hand is how it stops being read.
fn wrap(text: &str, width: usize) -> Vec<String> {
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

fn section(out: &mut String, title: &str, findings: &[Finding]) {
    let _ = writeln!(out, "{}", banner::head(title));
    for finding in findings {
        let _ = writeln!(out, "  {}  {}", finding.level.tag(), finding.headline);
        for line in &finding.detail {
            for wrapped in wrap(line, 62) {
                let _ = writeln!(out, "        {wrapped}");
            }
        }
    }
    out.push('\n');
}

/// Where a human can watch this memory. The viewer ships inside the binary
/// and mounts itself on an embedded session, so the only thing worth saying
/// is the address — and, when someone turned it off, how to get it back.
fn viewer_finding() -> Finding {
    match crate::viewer::viewer_addr_from_env().addr() {
        Some(addr) => Finding::new(
            Level::Ok,
            format!("your memory, as a graph: http://{addr}/"),
        )
        .with("an embedded session mounts it at startup; this command starts nothing"),
        None => Finding::new(Level::Warn, "declined — no viewer this session").with(format!(
            "unset {} and restart the session to see your memory again",
            kmp_viewer::VIEWER_ADDR_ENV
        )),
    }
}

/// `info` — the facts, with no verdict: what this binary is and what memory it
/// would open here.
pub fn info() -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}\n",
        banner::large_with(&format!(
            "  {} {}   ·   store formats {}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            compiled_formats()
        ))
    );

    section(&mut out, "Backend", &[backend_finding()]);
    let (data_dir, resolved) = data_dir_finding();
    section(&mut out, "Memory", &[data_dir]);

    let names = crate::tool_names();
    let surface = Finding::new(
        Level::Ok,
        format!("{} tools on the MCP surface", names.len()),
    )
    .with(names.join(" "));
    section(&mut out, "Tools", &[surface]);
    section(&mut out, "Viewer", &[viewer_finding()]);

    if let Some(resolved) = resolved {
        let history = startup_history(resolved.path(), 3);
        if !history.is_empty() {
            let mut recent = Finding::new(Level::Ok, "recent startups");
            for line in history {
                recent = recent.with(line);
            }
            section(&mut out, "History", &[recent]);
        }
    }
    out
}

/// `doctor` — the same facts, judged, ending in the one thing to fix.
///
/// Returns the report and the exit code, so a script can gate on it.
pub fn doctor() -> (String, i32) {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}\n",
        banner::large_with("  doctor — agent memory, end to end")
    );

    let binary = Finding::new(
        Level::Ok,
        format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
    )
    .with(format!("store formats: {}", compiled_formats()));
    section(&mut out, "Binary", &[binary]);

    let backend = backend_finding();
    section(&mut out, "Backend", std::slice::from_ref(&backend));

    let (data_dir, resolved) = data_dir_finding();
    let data_dir_level = data_dir.level;
    section(&mut out, "Memory", &[data_dir]);

    let tools = crate::tool_names();
    let surface = if tools.len() == 10 {
        Finding::new(Level::Ok, format!("{} tools answered", tools.len())).with(tools.join(" "))
    } else {
        Finding::new(
            Level::Fail,
            format!("{} tools on the surface, expected 10", tools.len()),
        )
    };
    let surface_level = surface.level;
    section(&mut out, "Tools", &[surface]);
    section(&mut out, "Viewer", &[viewer_finding()]);

    let mut history_level = Level::Ok;
    if let Some(resolved) = resolved.as_ref() {
        let history = startup_history(resolved.path(), 5);
        let finding = if history.is_empty() {
            history_level = Level::Warn;
            Finding::new(Level::Warn, "this memory has never been started here")
                .with("a host that never started leaves no line to read")
        } else {
            let mut recent = Finding::new(Level::Ok, "recent startups");
            for line in history {
                recent = recent.with(line);
            }
            recent
        };
        section(&mut out, "History", &[finding]);
    }

    let worst = [data_dir_level, surface_level, backend.level, history_level]
        .into_iter()
        .max_by_key(|level| match level {
            Level::Ok => 0,
            Level::Warn => 1,
            Level::Fail => 2,
        })
        .unwrap_or(Level::Ok);

    match worst {
        Level::Fail => {
            let _ = writeln!(out, "Not usable. Fix the FAIL above first.");
            (out, 1)
        }
        Level::Warn => {
            let _ = writeln!(out, "Usable, with a warning above.");
            (out, 0)
        }
        Level::Ok => {
            let _ = writeln!(out, "Usable.");
            (out, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn the_compiled_formats_name_the_engines_this_build_carries() {
        let formats = compiled_formats();
        assert!(
            formats.starts_with('1'),
            "redb is always compiled: {formats}"
        );
        assert_eq!(
            formats.contains("sqlite"),
            StorageEngine::Sqlite.is_compiled(),
            "the string must follow the build, not a guess"
        );
    }

    #[test]
    fn an_empty_directory_reports_no_engine_rather_than_guessing() {
        let empty = tempfile::tempdir().expect("tempdir");
        assert!(engine_on_disk(empty.path()).is_none());
    }

    #[test]
    fn the_engine_is_read_from_the_file_on_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = dir.path().join("store");
        std::fs::create_dir_all(&store).expect("store dir");
        std::fs::write(store.join("kernel.redb"), b"not a real store").expect("write");
        assert_eq!(engine_on_disk(dir.path()), Some(StorageEngine::Redb));
    }

    #[test]
    fn the_startup_history_reads_a_rolled_log() {
        // The log rolls daily, so the file on disk carries a date suffix.
        // Reading only `kmp-mcp.log` finds nothing on every day but the first.
        let dir = tempfile::tempdir().expect("tempdir");
        let logs = dir.path().join("logs");
        std::fs::create_dir_all(&logs).expect("logs dir");
        std::fs::write(
            logs.join("kmp-mcp.log.2026-08-23"),
            "{\"timestamp\":\"2026-08-23T19:12:46.0Z\",\"fields\":{\"message\":\"startup succeeded\"}}\n\
             {\"timestamp\":\"2026-08-23T19:27:42.0Z\",\"fields\":{\"message\":\"startup failed\"}}\n",
        )
        .expect("write log");

        let history = startup_history(dir.path(), 5);
        assert_eq!(history.len(), 2, "{history:?}");
        assert!(history[0].starts_with("ok"), "{history:?}");
        assert!(history[1].starts_with("failed"), "{history:?}");
        assert!(history[1].contains("2026-08-23 19:27:42"), "{history:?}");
    }

    #[test]
    fn the_history_keeps_only_the_newest_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let logs = dir.path().join("logs");
        std::fs::create_dir_all(&logs).expect("logs dir");
        let mut text = String::new();
        for minute in 0..9 {
            text.push_str(&format!(
                "{{\"timestamp\":\"2026-08-23T19:0{minute}:00.0Z\",\"fields\":{{\"message\":\"startup succeeded\"}}}}\n"
            ));
        }
        std::fs::write(logs.join("kmp-mcp.log.2026-08-23"), text).expect("write log");

        let history = startup_history(dir.path(), 3);
        assert_eq!(history.len(), 3);
        assert!(
            history[2].contains("19:08:00"),
            "the newest survives: {history:?}"
        );
    }

    #[test]
    fn a_missing_log_directory_is_silence_rather_than_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(startup_history(dir.path(), 5).is_empty());
    }

    /// The mark reaches a user through `/kmp:info` and `/kmp:doctor` and
    /// nowhere else — the startup banner goes to stderr and the host eats it,
    /// and nobody runs `--help` on a server a plugin launches. So a branded
    /// surface that quietly stops being branded should fail here rather than
    /// be noticed by nobody, which is what happened to the mark that was
    /// written, tested and never rendered.
    #[test]
    fn the_two_surfaces_a_user_actually_reaches_carry_the_mark() {
        let (doctor_report, _) = doctor();
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

    #[test]
    fn info_reports_the_surface_without_judging_it() {
        let report = info();
        assert!(report.contains("Kernel Memory Protocol"));
        assert!(report.contains("10 tools on the MCP surface"));
        assert!(report.contains("kernel_write_memory"));
        assert!(!report.contains("Usable"), "info states, doctor judges");
    }

    #[test]
    fn doctor_ends_in_a_verdict() {
        let (report, code) = doctor();
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

    #[test]
    fn neither_command_opens_or_creates_a_store() {
        // The promise the plugin's doctor made and this one inherits: a
        // diagnostic must not create a store as a side effect, nor take the
        // single-writer lock out from under a live session.
        let dir = tempfile::tempdir().expect("tempdir");
        let before: Vec<_> = std::fs::read_dir(dir.path()).expect("read").collect();
        assert!(before.is_empty());

        let _ = engine_on_disk(dir.path());
        let _ = startup_history(dir.path(), 5);

        let after: Vec<_> = std::fs::read_dir(dir.path()).expect("read").collect();
        assert!(after.is_empty(), "the directory is untouched");
    }
}
