//! `info` and `doctor`: what this binary is, and why the memory is not answering.
//!
//! Both read the layout from the filesystem and never open the store. A
//! diagnostic that creates a store, or contends with a live session for its
//! engine, has changed the thing it was asked to describe.

use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;

use kmp_embedded::{OrphanedProjectBundle, ResolvedDataDir, StorageEngine};

use crate::banner;
use crate::style::{self, Style};

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

    /// The tag's ink. The words already carry the verdict; on a terminal the
    /// color lets a reader find the one line that matters without reading.
    fn sgr(self) -> &'static str {
        match self {
            Level::Ok => style::OK,
            Level::Warn => style::WARN,
            Level::Fail => style::FAIL,
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
    format!("{} (sqlite)", StorageEngine::Sqlite.format_version())
}

/// Whether the compiled SQLite engine's store file is on disk.
pub fn engine_on_disk(data_dir: &Path) -> Option<StorageEngine> {
    kmp_embedded::store_file_path_for(data_dir, StorageEngine::Sqlite)
        .exists()
        .then_some(StorageEngine::Sqlite)
}

fn store_file_on_disk(data_dir: &Path) -> Option<PathBuf> {
    let sqlite = kmp_embedded::store_file_path_for(data_dir, StorageEngine::Sqlite);
    if sqlite.exists() {
        return Some(sqlite);
    }
    let mut artifacts = std::fs::read_dir(data_dir.join("store"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    artifacts.sort();
    artifacts.into_iter().next()
}

fn describe_data_dir(resolved: &ResolvedDataDir) -> Finding {
    let path = resolved.path();
    let layout = kmp_embedded::validate_store_layout(path);
    let mut finding = match &layout {
        Ok(_) => Finding::new(Level::Ok, path.display().to_string()),
        Err(error) => Finding::new(Level::Fail, "the selected memory cannot be opened")
            .with(format!("data dir: {}", path.display()))
            .with(error.to_string())
            .with("the diagnostic left every store file untouched"),
    }
    .with(format!("chosen by: {}", resolved.rule_name()));

    match layout {
        Ok(Some(engine)) => {
            finding = finding.with(format!("store format: {}", engine.format_version()));
        }
        Ok(None) => finding = finding.with("store format: not stamped yet"),
        Err(_) => {}
    }
    match engine_on_disk(path) {
        Some(engine) => finding = finding.with(format!("engine on disk: {engine}")),
        None => match store_file_on_disk(path) {
            Some(artifact) => {
                finding = finding
                    .with("storage artifact on disk: unsupported; source left untouched")
                    .with(format!("artifact: {}", artifact.display()));
            }
            None => finding = finding.with("no store yet — it is created on first write"),
        },
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

/// Whether the machine state has a current, verifiable git-native copy. The
/// store itself is never opened: the pending marker brackets every write, and
/// file modification times catch stores written by older binaries that did
/// not create one.
fn committed_bundle_finding(resolved: &ResolvedDataDir) -> Option<Finding> {
    if let Some(orphaned) = resolved.orphaned_bundle() {
        return Some(orphaned_bundle_finding(orphaned));
    }
    let bundle = kmp_embedded::project_bundle_path(resolved)?;
    let store = store_file_on_disk(resolved.path());
    let pending = kmp_embedded::pending_bundle_exports(resolved.path());
    if !pending.is_empty() {
        let mut finding = Finding::new(
            Level::Fail,
            format!(
                "{} write {} not proved in the committed bundle",
                pending.len(),
                if pending.len() == 1 { "is" } else { "are" }
            ),
        )
        .with(format!("bundle: {}", bundle.display()))
        .with(
            "the store may contain a write whose process stopped before export completed; stop \
             other KMP sessions, run `kmp-mcp export`, inspect the diff, then run `kmp-mcp \
             export --repair-pending` and commit it",
        );
        for marker in pending {
            finding = finding.with(format!("pending: {}", marker.display()));
        }
        return Some(finding);
    }

    if !bundle.exists() {
        return Some(if store.is_some() {
            Finding::new(Level::Fail, "memory exists only in the gitignored store")
                .with(format!("missing: {}", bundle.display()))
                .with("run `kmp-mcp export`, inspect the diff, and commit it")
        } else {
            Finding::new(Level::Ok, "no memory to protect yet").with(format!(
                "the first write will maintain {}",
                bundle.display()
            ))
        });
    }

    let text = match std::fs::read_to_string(&bundle) {
        Ok(text) => text,
        Err(error) => {
            return Some(
                Finding::new(Level::Fail, "the committed memory cannot be read")
                    .with(format!("{}: {error}", bundle.display())),
            );
        }
    };
    let header = match kmp_embedded::verify_bundle(&text) {
        Ok(header) => header,
        Err(error) => {
            return Some(
                Finding::new(Level::Fail, "the committed memory does not verify")
                    .with(format!("{}: {error}", bundle.display()))
                    .with("do not restore it; regenerate with `kmp-mcp export` first"),
            );
        }
    };
    if store.is_some() {
        let live = kmp_embedded::EmbeddedKernelStore::open(resolved.path())
            .and_then(|store| store.export_bundle_blocking());
        let live = match live {
            Ok(live) => live,
            Err(error) => {
                return Some(
                    Finding::new(Level::Fail, "the live memory cannot be audited")
                        .with(error.to_string()),
                );
            }
        };
        let live_header = match kmp_embedded::verify_bundle(&live) {
            Ok(header) => header,
            Err(error) => {
                return Some(
                    Finding::new(Level::Fail, "the live memory export does not verify")
                        .with(error.to_string()),
                );
            }
        };
        if let Err(error) = kmp_embedded::merge_bundles(&text, &live, "doctor-history-audit") {
            return Some(
                Finding::new(
                    Level::Fail,
                    "the live store and committed memory are divergent histories",
                )
                .with(error.to_string())
                .with("reconcile them explicitly; do not restore an archived machine history"),
            );
        }
        if header.event_count != live_header.event_count
            || header.content_digest != live_header.content_digest
        {
            return Some(
                Finding::new(
                    Level::Fail,
                    "the live store and committed memory are different revisions",
                )
                .with(format!(
                    "live events: {}; committed events: {}",
                    live_header.event_count, header.event_count
                ))
                .with("run `kmp-mcp export`, inspect the diff, and reconcile explicitly"),
            );
        }
    }
    if let Some(store) = store
        && newer_than(&store, &bundle)
    {
        return Some(
            Finding::new(
                Level::Fail,
                "the gitignored store is newer than its committed memory",
            )
            .with(format!("store:  {}", store.display()))
            .with(format!("bundle: {}", bundle.display()))
            .with("run `kmp-mcp export`, inspect the diff, and commit it"),
        );
    }
    if header.bundle_format < kmp_embedded::BUNDLE_FORMAT_VERSION {
        return Some(
            Finding::new(
                Level::Warn,
                "the committed memory uses a legacy bundle header",
            )
            .with(format!(
                "{} events · no snapshot identity or digest",
                header.event_count
            ))
            .with("run `kmp-mcp export` to upgrade it without changing the events"),
        );
    }

    Some(
        Finding::new(
            Level::Ok,
            format!(
                "snapshot {} protects {} {}",
                header.snapshot_id,
                header.event_count,
                if header.event_count == 1 {
                    "event"
                } else {
                    "events"
                }
            ),
        )
        .with(format!("bundle: {}", bundle.display()))
        .with(format!("digest: {}", header.content_digest))
        .with(format!("abouts: {}", header.abouts.join(" "))),
    )
}

fn orphaned_bundle_finding(orphaned: &OrphanedProjectBundle) -> Finding {
    let mut finding = Finding::new(
        Level::Fail,
        "this project's committed memory is no longer being maintained",
    );
    let bundle_detail = match std::fs::read_to_string(&orphaned.bundle_path) {
        Ok(bundle) => match kmp_embedded::verify_bundle(&bundle) {
            Ok(header) => format!(
                "bundle: {} (last event {}, snapshot time {} ms)",
                orphaned.bundle_path.display(),
                header.event_count,
                header.created_at_unix_ms
            ),
            Err(error) => format!(
                "bundle: {} (cannot verify: {error})",
                orphaned.bundle_path.display()
            ),
        },
        Err(error) => format!(
            "bundle: {} (cannot read: {error})",
            orphaned.bundle_path.display()
        ),
    };
    finding = finding
        .with(bundle_detail)
        .with(format!(
            "project store: {} — not selected: {}",
            orphaned.project_store_path.display(),
            orphaned.reason
        ))
        .with(format!(
            "writes are going to: {}",
            orphaned.selected_store_path.display()
        ));

    let abouts = std::fs::read_to_string(&orphaned.bundle_path)
        .ok()
        .and_then(|bundle| kmp_embedded::verify_bundle(&bundle).ok())
        .map(|header| header.abouts)
        .unwrap_or_default();
    if let [about] = abouts.as_slice() {
        finding.with(format!(
            "automatic maintenance resumes only when the project store opens again; until then, \
             refresh this bundle explicitly with `kmp-mcp export {} --about {about}`",
            orphaned.bundle_path.display()
        ))
    } else {
        finding.with(
            "automatic maintenance resumes only when the project store opens again; a filtered \
             explicit export can refresh known abouts safely",
        )
    }
}

fn newer_than(left: &Path, right: &Path) -> bool {
    let modified = |path: &Path| std::fs::metadata(path).and_then(|metadata| metadata.modified());
    matches!((modified(left), modified(right)), (Ok(left), Ok(right)) if left > right)
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

fn section(out: &mut String, style: Style, title: &str, findings: &[Finding]) {
    let _ = writeln!(out, "{}", banner::head_styled(style, title));
    for finding in findings {
        let _ = writeln!(
            out,
            "  {}  {}",
            style.paint(finding.level.sgr(), finding.level.tag()),
            finding.headline
        );
        for line in &finding.detail {
            for wrapped in wrap(line, 62) {
                let _ = writeln!(out, "        {wrapped}");
            }
        }
    }
    out.push('\n');
}

/// Where all the memory on this machine is, not only the one this shell would
/// open. Two of five stores on a real machine were reachable by no rule at
/// all, and nothing that shipped would ever have mentioned them.
fn memories_finding() -> Vec<Finding> {
    let Some(data_home) = kmp_embedded::user_data_home() else {
        return vec![
            Finding::new(Level::Warn, "cannot tell what memory is here").with(
                "none of XDG_DATA_HOME, HOME, LOCALAPPDATA, APPDATA, or USERPROFILE is set, \
                     so there is nowhere to look",
            ),
        ];
    };
    let index = data_home.join("kmp").join(crate::memories::INDEX_FILE);
    let memories = crate::memories::list(&data_home, &crate::memories::read_index(&index));
    if memories.is_empty() {
        return vec![
            Finding::new(Level::Ok, "no memory on this machine yet")
                .with("the first write creates one; where depends on where you are standing"),
        ];
    }

    let opening = kmp_embedded::locate_data_dir_from_env()
        .ok()
        .map(|resolved| resolved.path().to_path_buf());
    let unreachable = memories
        .iter()
        .filter(|memory| memory.reach == crate::memories::Reach::Unreachable)
        .count();

    let mut finding = Finding::new(
        if unreachable > 0 {
            Level::Warn
        } else {
            Level::Ok
        },
        format!(
            "{} {} on this machine{}",
            memories.len(),
            if memories.len() == 1 {
                "memory"
            } else {
                "memories"
            },
            if unreachable > 0 {
                format!(", {unreachable} that no rule reaches")
            } else {
                String::new()
            }
        ),
    );
    for memory in &memories {
        let here = opening.as_deref() == Some(memory.path.as_path());
        finding = finding.with(format!(
            "{} {} · {} · {}{}{}",
            if here { "→" } else { " " },
            memory.path.display(),
            crate::memories::human_size(memory.bytes),
            memory.reach.as_str(),
            memory
                .storage
                .as_deref()
                .map(|storage| format!(" · {storage}"))
                .unwrap_or_default(),
            memory
                .last_opened
                .as_deref()
                .map(|when| format!(" · last opened {when}"))
                .unwrap_or_default()
        ));
    }
    if unreachable > 0 {
        finding = finding.with(
            "`unreachable` means no rule resolves to it: open it with KMP_MCP_DATA_DIR, or \
             remove exactly it with `kmp-mcp uninstall --store <absolute path>`, which refuses \
             live owners and saves the memory first",
        );
    }
    vec![finding]
}

/// Where a human can watch this memory. The capability belongs to the running
/// session, so this separate diagnostic never prints a bare URL that will 401.
fn viewer_finding() -> Finding {
    match crate::viewer::viewer_addr_from_env().addr() {
        Some(_) => Finding::new(Level::Ok, "ChronoLoom comes with an embedded session")
            .with("ask the agent to open it — only that session knows its capability link"),
        None => Finding::new(Level::Warn, "declined — no viewer this session").with(format!(
            "unset {} and restart the session to see your memory again",
            kmp_viewer::VIEWER_ADDR_ENV
        )),
    }
}

fn telemetry_finding(resolved: &ResolvedDataDir) -> Finding {
    let path = kmp_embedded::quality_telemetry_path(resolved.path());
    if !path.exists() {
        return Finding::new(Level::Warn, "no quality telemetry journal yet").with(format!(
            "expected at {} after the first kernel start",
            path.display()
        ));
    }
    match kmp_embedded::SqliteQualityTelemetryReader::open(resolved.path()) {
        Ok(reader) => match reader.count() {
            Ok(count) => Finding::new(
                Level::Ok,
                format!("quality pulse readable · {count} observations"),
            )
            .with(path.display().to_string()),
            Err(error) => Finding::new(Level::Warn, "quality telemetry cannot be read")
                .with(error.to_string()),
        },
        Err(error) => {
            let raw = error.to_string();
            let headline = if raw.contains("Cannot acquire lock")
                || raw.to_ascii_lowercase().contains("already open")
            {
                "quality telemetry is held by another process"
            } else {
                "quality telemetry is unavailable"
            };
            Finding::new(Level::Warn, headline).with(raw)
        }
    }
}

fn agent_policy_finding() -> Finding {
    match crate::agent_policy::load() {
        Ok(policy) => {
            let languages = if policy.ask_fallback_languages.is_empty() {
                "none".to_string()
            } else {
                policy.ask_fallback_languages.join(", ")
            };
            Finding::new(
                Level::Ok,
                format!(
                    "semantic Ask fallback: {languages} ({})",
                    policy.source_label()
                ),
            )
            .with(format!("config: {}", policy.path.display()))
            .with("temporal intent bypasses Ask and navigates time first")
        }
        Err(error) => Finding::new(Level::Warn, "agent policy is invalid")
            .with(error)
            .with("repair it with `kmp-mcp config ask-fallback-languages en`"),
    }
}

/// `info` — the facts, with no verdict: what this binary is and what memory it
/// would open here. Styled for whatever stdout is: a pipe gets the pinned
/// plain bytes, a terminal gets ink.
pub fn info() -> String {
    info_styled(Style::for_stdout())
}

fn info_styled(style: Style) -> String {
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
    let surface = Finding::new(
        Level::Ok,
        format!("{} tools on the MCP surface", names.len()),
    )
    .with(names.join(" "));
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
            let mut recent = Finding::new(Level::Ok, "recent startups");
            for line in history {
                recent = recent.with(line);
            }
            section(&mut out, style, "History", &[recent]);
        }
    }
    out
}

/// `doctor` — the same facts, judged, ending in the one thing to fix.
///
/// Returns the report and the exit code, so a script can gate on it.
pub fn doctor() -> (String, i32) {
    doctor_styled(Style::for_stdout())
}

fn lifecycle_findings() -> Vec<Finding> {
    use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;

    crate::lifecycle::NativeLifecycle::diagnose()
        .findings()
        .iter()
        .map(|finding| {
            let level = match finding.severity() {
                DiagnosticSeverity::Ok => Level::Ok,
                DiagnosticSeverity::Warn => Level::Warn,
                DiagnosticSeverity::Fail => Level::Fail,
            };
            finding.detail().iter().fold(
                Finding::new(level, finding.headline()),
                |rendered, detail| rendered.with(detail),
            )
        })
        .collect()
}

fn doctor_styled(style: Style) -> (String, i32) {
    doctor_styled_with_lifecycle(style, lifecycle_findings())
}

fn doctor_styled_with_lifecycle(style: Style, lifecycle: Vec<Finding>) -> (String, i32) {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}\n",
        banner::large_with(style, "  doctor — agent memory, end to end")
    );

    let binary = Finding::new(
        Level::Ok,
        format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
    )
    .with(format!("store formats: {}", compiled_formats()));
    section(&mut out, style, "Binary", &[binary]);

    let backend = backend_finding();
    section(&mut out, style, "Backend", std::slice::from_ref(&backend));

    let (data_dir, resolved) = data_dir_finding();
    let data_dir_level = data_dir.level;
    section(&mut out, style, "Memory", &[data_dir]);
    let durability = resolved.as_ref().and_then(committed_bundle_finding);
    let durability_level = durability
        .as_ref()
        .map_or(Level::Ok, |finding| finding.level);
    if let Some(durability) = durability {
        section(&mut out, style, "Durability", &[durability]);
    }

    let tools = crate::tool_names();
    let surface = tool_surface_finding(&tools, &crate::protocol::declared_tool_names());
    let surface_level = surface.level;
    section(&mut out, style, "Tools", &[surface]);
    let lifecycle_level = lifecycle
        .iter()
        .map(|finding| finding.level)
        .max_by_key(|level| match level {
            Level::Ok => 0,
            Level::Warn => 1,
            Level::Fail => 2,
        })
        .unwrap_or(Level::Ok);
    section(&mut out, style, "Hosts", &lifecycle);
    let agent_policy = agent_policy_finding();
    let agent_policy_level = agent_policy.level;
    section(&mut out, style, "Agent", &[agent_policy]);
    section(&mut out, style, "Viewer", &[viewer_finding()]);
    let telemetry = resolved.as_ref().map(telemetry_finding);
    let telemetry_level = telemetry
        .as_ref()
        .map_or(Level::Ok, |finding| finding.level);
    if let Some(telemetry) = telemetry {
        section(&mut out, style, "Telemetry", &[telemetry]);
    }

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
        section(&mut out, style, "History", &[finding]);
    }

    let worst = [
        data_dir_level,
        durability_level,
        surface_level,
        lifecycle_level,
        backend.level,
        history_level,
        agent_policy_level,
        telemetry_level,
    ]
    .into_iter()
    .max_by_key(|level| match level {
        Level::Ok => 0,
        Level::Warn => 1,
        Level::Fail => 2,
    })
    .unwrap_or(Level::Ok);

    // The verdict wears the worst finding's ink: the one line a reader
    // scrolls to is the one line that should be findable at a glance.
    match worst {
        Level::Fail => {
            let _ = writeln!(
                out,
                "{}",
                style.paint(Level::Fail.sgr(), "Not usable. Fix the FAIL above first.")
            );
            (out, 1)
        }
        Level::Warn => {
            let _ = writeln!(
                out,
                "{}",
                style.paint(Level::Warn.sgr(), "Usable, with a warning above.")
            );
            (out, 0)
        }
        Level::Ok => {
            let _ = writeln!(out, "{}", style.paint(Level::Ok.sgr(), "Usable."));
            (out, 0)
        }
    }
}

fn tool_surface_finding(observed: &[String], declared: &[String]) -> Finding {
    let observed_names = observed
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let declared_names = declared
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let missing = declared_names
        .difference(&observed_names)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected = observed_names
        .difference(&declared_names)
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() && unexpected.is_empty() {
        Finding::new(
            Level::Ok,
            format!("{} declared tools answered", observed.len()),
        )
        .with(observed.join(" "))
    } else {
        let mut finding = Finding::new(
            Level::Fail,
            "the MCP tool surface differs from its protocol",
        );
        if !missing.is_empty() {
            finding = finding.with(format!("missing: {}", missing.join(" ")));
        }
        if !unexpected.is_empty() {
            finding = finding.with(format!("unexpected: {}", unexpected.join(" ")));
        }
        finding
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lifecycle_fixture() -> Vec<Finding> {
        vec![Finding::new(
            Level::Ok,
            "native hosts use the tested lifecycle",
        )]
    }

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
        assert!(!formats.contains("legacy read"), "{formats}");
        assert!(formats.contains("2 (sqlite)"), "{formats}");
    }

    #[test]
    fn an_empty_directory_reports_no_engine_rather_than_guessing() {
        let empty = tempfile::tempdir().expect("tempdir");
        assert!(engine_on_disk(empty.path()).is_none());
    }

    #[test]
    fn an_unopenable_layout_is_a_failure_and_never_claims_memory_is_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = kmp_embedded::store_file_path_for(dir.path(), StorageEngine::Sqlite);
        std::fs::create_dir_all(store.parent().expect("parent")).expect("store dir");
        std::fs::write(&store, b"memory is still present").expect("store marker");
        let resolved = ResolvedDataDir::Explicit(dir.path().to_path_buf());

        for stamp in [Some("3\n"), Some("banana\n"), None] {
            match stamp {
                Some(stamp) => std::fs::write(kmp_embedded::format_version_path(dir.path()), stamp)
                    .expect("write invalid stamp"),
                None => std::fs::remove_file(kmp_embedded::format_version_path(dir.path()))
                    .expect("remove stamp"),
            }
            let finding = describe_data_dir(&resolved);
            assert_eq!(finding.level, Level::Fail, "{finding:?}");
            assert!(finding.headline.contains("cannot be opened"), "{finding:?}");
            assert!(
                finding
                    .detail
                    .iter()
                    .any(|line| line == "engine on disk: sqlite"),
                "{finding:?}"
            );
            assert!(
                finding
                    .detail
                    .iter()
                    .all(|line| !line.contains("no store yet")),
                "{finding:?}"
            );
            assert!(store.exists(), "diagnosis must preserve the memory file");
        }
    }

    #[test]
    fn an_unknown_storage_artifact_is_preserved_without_naming_an_engine() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = dir.path().join("store");
        std::fs::create_dir_all(&store).expect("store dir");
        std::fs::write(store.join("retired-layout.bin"), b"not a real store").expect("write");
        assert!(engine_on_disk(dir.path()).is_none());
        assert_eq!(
            store_file_on_disk(dir.path()),
            Some(store.join("retired-layout.bin"))
        );
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

    #[test]
    fn a_project_store_without_a_committed_copy_is_a_durability_failure() {
        let project = tempfile::tempdir().expect("project");
        let data_dir = project.path().join(".kernel");
        let store = data_dir.join("store");
        std::fs::create_dir_all(&store).expect("store dir");
        std::fs::write(store.join("retired-layout.bin"), b"store").expect("store marker");
        let resolved = ResolvedDataDir::Project(data_dir);

        let finding = committed_bundle_finding(&resolved).expect("project finding");
        assert_eq!(finding.level, Level::Fail);
        assert!(finding.headline.contains("only in the gitignored store"));
        assert!(
            finding
                .detail
                .iter()
                .any(|line| line.contains("kmp-mcp export"))
        );
    }

    #[test]
    fn a_pending_write_is_louder_than_an_existing_bundle() {
        let project = tempfile::tempdir().expect("project");
        let data_dir = project.path().join(".kernel");
        let pending = data_dir.join(kmp_embedded::PENDING_EXPORT_DIR);
        std::fs::create_dir_all(&pending).expect("pending dir");
        std::fs::write(pending.join("write.pending"), b"pending").expect("marker");
        let resolved = ResolvedDataDir::Project(data_dir);

        let finding = committed_bundle_finding(&resolved).expect("project finding");
        assert_eq!(finding.level, Level::Fail);
        assert!(finding.headline.contains("not proved"));
        assert!(finding.detail.iter().any(|line| line.contains("pending:")));
    }

    #[test]
    fn a_legacy_bundle_is_readable_but_not_mistaken_for_an_identified_snapshot() {
        let project = tempfile::tempdir().expect("project");
        let data_dir = project.path().join(".kernel");
        let bundle = project.path().join(".kmp/memory.jsonl");
        std::fs::create_dir_all(bundle.parent().expect("parent")).expect("bundle dir");
        std::fs::write(
            bundle,
            r#"{"bundle_format":1,"store_format":1,"event_count":0,"kernel_version":"0.1.3"}"#,
        )
        .expect("legacy bundle");
        let resolved = ResolvedDataDir::Project(data_dir);

        let finding = committed_bundle_finding(&resolved).expect("project finding");
        assert_eq!(finding.level, Level::Warn);
        assert!(finding.headline.contains("legacy"));
    }

    #[test]
    fn an_orphaned_project_bundle_is_compared_with_the_store_that_receives_writes() {
        let project = tempfile::tempdir().expect("project");
        let bundle = project.path().join(".kmp/memory.jsonl");
        std::fs::create_dir_all(bundle.parent().expect("bundle parent")).expect("bundle dir");
        std::fs::write(
            &bundle,
            r#"{"bundle_format":1,"store_format":1,"event_count":0,"kernel_version":"0.2.4"}"#,
        )
        .expect("legacy bundle");
        let project_store = project.path().join(".kernel");
        let selected_store = project.path().join("user-store");
        let resolved = ResolvedDataDir::UserFallback {
            path: selected_store.clone(),
            orphaned_bundle: OrphanedProjectBundle {
                bundle_path: bundle.clone(),
                project_store_path: project_store.clone(),
                selected_store_path: selected_store.clone(),
                reason: "store format 1 is retired".to_string(),
            },
        };

        let finding = committed_bundle_finding(&resolved).expect("orphan finding");
        assert_eq!(finding.level, Level::Fail);
        assert!(finding.headline.contains("no longer being maintained"));
        assert!(
            finding
                .detail
                .iter()
                .any(|line| line.contains(&bundle.display().to_string()))
        );
        assert!(
            finding
                .detail
                .iter()
                .any(|line| line.contains(&project_store.display().to_string())
                    && line.contains("not selected"))
        );
        assert!(finding.detail.iter().any(|line| {
            line.contains("writes are going to")
                && line.contains(&selected_store.display().to_string())
        }));
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

    #[test]
    fn info_reports_the_surface_without_judging_it() {
        let report = info();
        assert!(report.contains("Kernel Memory Protocol"));
        assert!(report.contains("13 tools on the MCP surface"));
        assert!(report.contains("kmp_write_memory"));
        assert!(!report.contains("Usable"), "info states, doctor judges");
    }

    #[test]
    fn diagnostics_never_offer_an_unauthorised_viewer_url() {
        let finding = viewer_finding();
        assert!(
            !finding.headline.contains("http://")
                && finding.detail.iter().all(|line| !line.contains("http://")),
            "a separate process cannot know the running session's capability: {finding:?}"
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

    #[test]
    fn the_tool_finding_reports_drift_by_name_not_by_count() {
        let observed = vec!["kmp_wake".to_string(), "kmp_surprise".to_string()];
        let declared = vec!["kmp_wake".to_string(), "kmp_ask".to_string()];
        let finding = tool_surface_finding(&observed, &declared);
        assert_eq!(finding.level, Level::Fail);
        assert!(finding.detail.iter().any(|line| line == "missing: kmp_ask"));
        assert!(
            finding
                .detail
                .iter()
                .any(|line| line == "unexpected: kmp_surprise")
        );
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
