//! Named, read-only recovery points over commit-native event bundles.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use kmp_application::projection_mutations_for_context_event;
use kmp_embedded::{BundleHeader, ResolvedDataDir};
use serde_json::{Value, json};

use crate::KernelMcpServer;

pub const PROJECT_SNAPSHOTS_PATH: &str = ".kmp/snapshots";

static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("snapshot name must not be empty".to_string());
    }
    if name == "." || name == ".." {
        return Err("snapshot name must identify one file, not a directory".to_string());
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(format!(
            "snapshot name `{name}` may contain only letters, digits, dot, underscore and dash"
        ));
    }
    Ok(())
}

pub fn path_for_name(resolved: &ResolvedDataDir, name: &str) -> Result<PathBuf, String> {
    validate_name(name)?;
    let Some(head) = kmp_embedded::project_bundle_path(resolved) else {
        return Err(format!(
            "named snapshots belong to a project, but this store resolved to `{}` by the `{}` \
             rule; use a project-scoped store or the explicit `export <file>` command",
            resolved.path().display(),
            resolved.rule_name()
        ));
    };
    let project_root = head
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "project bundle path has no project root".to_string())?;
    Ok(project_root
        .join(PROJECT_SNAPSHOTS_PATH)
        .join(format!("{name}.jsonl")))
}

pub fn read_header(path: &Path) -> Result<BundleHeader, String> {
    let bundle = std::fs::read_to_string(path)
        .map_err(|error| format!("could not read snapshot `{}`: {error}", path.display()))?;
    kmp_embedded::verify_bundle(&bundle)
        .map_err(|error| format!("snapshot `{}` does not verify: {error}", path.display()))
}

pub fn list(resolved: &ResolvedDataDir) -> Result<Vec<(PathBuf, BundleHeader)>, String> {
    let probe = path_for_name(resolved, "probe")?;
    let Some(directory) = probe.parent() else {
        return Ok(Vec::new());
    };
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "could not list snapshot directory `{}`: {error}",
                directory.display()
            ));
        }
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| read_header(&path).map(|header| (path, header)))
        .collect()
}

/// Runs one existing read tool against a verified bundle imported into an
/// isolated temporary store. The process's live store is never opened, moved
/// or replaced.
pub async fn read_only(bundle: &str, tool: &str, arguments: Value) -> Result<Value, String> {
    const READ_TOOLS: &[&str] = &[
        "kmp_wake",
        "kmp_ask",
        "kmp_relate",
        "kmp_goto",
        "kmp_near",
        "kmp_rewind",
        "kmp_forward",
        "kmp_trace",
        "kmp_inspect",
    ];
    if !READ_TOOLS.contains(&tool) {
        return Err(format!(
            "snapshot read accepts only read tools ({}); `{tool}` could mutate memory",
            READ_TOOLS.join(", ")
        ));
    }
    kmp_embedded::verify_bundle(bundle)
        .map_err(|error| format!("snapshot does not verify: {error}"))?;

    let scratch = ScratchStore::new()?;
    let kernel = kmp_embedded::EmbeddedKernel::open(scratch.path())
        .map_err(|error| format!("could not open isolated snapshot store: {error}"))?;
    kernel
        .store()
        .import_bundle(bundle, projection_mutations_for_context_event)
        .await
        .map_err(|error| format!("could not attach snapshot read-only: {error}"))?;
    drop(kernel);

    let server = KernelMcpServer::embedded(scratch.path())?;
    let request = json!({
        "jsonrpc": "2.0",
        "id": "snapshot-read",
        "method": "tools/call",
        "params": {"name": tool, "arguments": arguments}
    });
    let response = server
        .handle_json_line(&request.to_string())
        .await
        .ok_or_else(|| "snapshot read returned no MCP response".to_string())?;
    serde_json::from_str(&response)
        .map_err(|error| format!("snapshot read returned invalid JSON: {error}"))
}

struct ScratchStore {
    path: PathBuf,
}

impl ScratchStore {
    fn new() -> Result<Self, String> {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos();
        let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "kmp-snapshot-read-{}-{time}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).map_err(|error| {
            format!(
                "could not create isolated snapshot directory `{}`: {error}",
                path.display()
            )
        })?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchStore {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_cannot_escape_the_snapshot_directory() {
        for rejected in ["", ".", "..", "../outside", "a/b", "a b"] {
            assert!(validate_name(rejected).is_err(), "{rejected}");
        }
        for accepted in ["pre-release", "v0.1.13", "release_2026"] {
            assert!(validate_name(accepted).is_ok(), "{accepted}");
        }
    }
}
