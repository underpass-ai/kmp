//! The configuration acknowledgement a store leaves when it opens.
//!
//! Every optional file the embedded backend consults beside a store —
//! `typesafe.json`, `rerank.json`, `wake-focus.json`, `write-relations.json`,
//! `curate.json`, `ask-gate.json`, `judgement-book.json`,
//! `semantic-retrieval.json`, the lexical bridge and the
//! judgement cassette —
//! is reported on target `kmp_mcp::store_config` with its sha256: at debug
//! when it took effect, at warn when it is present but ignored, with the
//! reason. Applied files are read and hashed only when debug is on for the
//! target (`RUST_LOG=kmp_mcp=info,kmp_mcp::store_config=debug`), so an
//! ordinary start does not hash a multi-megabyte lexical bridge. A `*.json` or `*.kmpb` file beside the store that this binary
//! does not read is reported as ignored too, so a configuration written for
//! another version cannot pass for applied. An ignored file is hashed at warn
//! only up to `IGNORED_HASH_LIMIT` bytes; a larger one reports its size with
//! an empty `sha256` unless debug is on, so a stray bridge left beside a store
//! costs no read at an ordinary start. The memory bench reads these
//! lines (`scripts/performance/memory_bench/SCHEMAS.md`, section telemetry).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const TARGET: &str = "kmp_mcp::store_config";

/// The largest ignored file hashed when debug is off: configuration files
/// are a few hundred bytes, a lexical bridge is megabytes.
pub(super) const IGNORED_HASH_LIMIT: u64 = 8 * 1024;

/// The optional files one store consulted, and whether each took effect.
pub(super) struct StoreConfigReport {
    data_dir: PathBuf,
    files: Vec<(&'static str, Option<PathBuf>, Result<(), String>)>,
}

impl StoreConfigReport {
    pub(super) fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            files: Vec::new(),
        }
    }

    /// A file named `name` beside the store; `verdict` is whether it applied
    /// when present, or why not.
    pub(super) fn beside_store(self, name: &'static str, verdict: Result<(), String>) -> Self {
        let path = self.data_dir.join(name);
        self.at(name, Some(path), verdict)
    }

    /// A file consulted at `path`, wherever it lives; `None` when the
    /// operator turned it off.
    pub(super) fn at(
        mut self,
        name: &'static str,
        path: Option<PathBuf>,
        verdict: Result<(), String>,
    ) -> Self {
        self.files.push((name, path, verdict));
        self
    }

    /// One line per present file, then one summary line. Files that took
    /// effect are reported, and hashed, only at debug.
    pub(super) fn emit(&self) {
        if !tracing::enabled!(target: TARGET, tracing::Level::WARN) {
            return;
        }
        let verbose = tracing::enabled!(target: TARGET, tracing::Level::DEBUG);
        let data_dir = self.data_dir.display().to_string();
        let mut loaded = Vec::<String>::new();
        let mut ignored = Vec::<String>::new();
        for (name, path, verdict) in &self.files {
            let Some(path) = path.as_deref().filter(|path| path.is_file()) else {
                continue;
            };
            let applied = verdict.as_ref().map_err(String::as_str).copied();
            if verbose || applied.is_err() {
                Self::file_line(&data_dir, name, path, applied, verbose);
            }
            match verdict {
                Ok(()) => loaded.push((*name).to_string()),
                Err(_) => ignored.push((*name).to_string()),
            }
        }
        for unread in self.unread() {
            let name = unread
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            Self::file_line(
                &data_dir,
                &name,
                &unread,
                Err("not read by this kmp-mcp version"),
                verbose,
            );
            ignored.push(name);
        }
        tracing::debug!(
            target: TARGET,
            event = "kmp_store_config_summary",
            data_dir = data_dir.as_str(),
            loaded = loaded.join(",").as_str(),
            ignored = ignored.join(",").as_str(),
            "store configuration read"
        );
    }

    fn file_line(
        data_dir: &str,
        name: &str,
        path: &Path,
        applied: Result<(), &str>,
        verbose: bool,
    ) {
        let (sha256, bytes) = Self::fingerprint(path, verbose || applied.is_ok());

        let path = path.display().to_string();
        match applied {
            Ok(()) => tracing::debug!(
                target: TARGET,
                event = "kmp_store_config",
                data_dir,
                file = name,
                path = path.as_str(),
                sha256 = sha256.as_str(),
                bytes,
                status = "loaded",
                "store configuration loaded"
            ),
            Err(reason) => tracing::warn!(
                target: TARGET,
                event = "kmp_store_config",
                data_dir,
                file = name,
                path = path.as_str(),
                sha256 = sha256.as_str(),
                bytes,
                status = "ignored",
                reason,
                "store configuration present but ignored"
            ),
        }
    }

    /// The file's sha256 and size. Without `hash_any_size` a file above
    /// `IGNORED_HASH_LIMIT` is not read: its size comes from the metadata and
    /// its hash is empty. A failed read reports `""` and 0.
    fn fingerprint(path: &Path, hash_any_size: bool) -> (String, u64) {
        if !hash_any_size {
            match std::fs::metadata(path) {
                Ok(metadata) if metadata.len() > IGNORED_HASH_LIMIT => {
                    return (String::new(), metadata.len());
                }
                Ok(_) => {}
                Err(_) => return (String::new(), 0),
            }
        }
        match std::fs::read(path) {
            Ok(bytes) => (format!("{:x}", Sha256::digest(&bytes)), bytes.len() as u64),
            Err(_) => (String::new(), 0),
        }
    }

    /// Configuration-shaped files beside the store that nothing consulted.
    fn unread(&self) -> Vec<PathBuf> {
        let Ok(listing) = std::fs::read_dir(&self.data_dir) else {
            return Vec::new();
        };
        let known = self
            .files
            .iter()
            .filter_map(|(_, path, _)| path.as_deref())
            .collect::<Vec<_>>();
        let mut unread = listing
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json" || extension == "kmpb")
            })
            .filter(|path| !known.contains(&path.as_path()))
            .collect::<Vec<_>>();
        unread.sort();
        unread
    }
}
