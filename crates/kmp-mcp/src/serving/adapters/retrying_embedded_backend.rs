use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use kmp_embedded::CommitNativeBundle;
use serde_json::Value;

use super::embedded_backend::EmbeddedKernelMcpBackend;
use crate::serving::{KernelMcpToolBackend, KernelMcpToolFuture};

/// Embedded backend that opens the SQLite store on the first memory call.
///
/// MCP discovery (`initialize` and `tools/list`) does not need the database.
/// Keeping that surface alive lets diagnostics and discovery remain available
/// even when the store layout itself is invalid or unsupported.
pub struct RetryingEmbeddedKernelMcpBackend {
    data_dir: PathBuf,
    engine: Option<kmp_embedded::StorageEngine>,
    commit_native: Option<CommitNativeBundle>,
    opened: Mutex<Option<Arc<EmbeddedKernelMcpBackend>>>,
}

impl RetryingEmbeddedKernelMcpBackend {
    pub fn new(data_dir: &Path, engine: Option<kmp_embedded::StorageEngine>) -> Self {
        Self::new_with_commit_native(data_dir, engine, None)
    }

    pub fn new_with_commit_native(
        data_dir: &Path,
        engine: Option<kmp_embedded::StorageEngine>,
        commit_native: Option<CommitNativeBundle>,
    ) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            engine,
            commit_native,
            opened: Mutex::new(None),
        }
    }

    /// Best engine label available without opening or stamping the store.
    pub fn declared_engine(&self) -> Option<kmp_embedded::StorageEngine> {
        let stamp = std::fs::read_to_string(self.data_dir.join("FORMAT_VERSION"))
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .and_then(|version| {
                (version == kmp_embedded::StorageEngine::Sqlite.format_version())
                    .then_some(kmp_embedded::StorageEngine::Sqlite)
            });
        stamp.or(self.engine)
    }

    fn opened_backend(&self) -> Result<Arc<EmbeddedKernelMcpBackend>, String> {
        if let Some(backend) = self
            .opened
            .lock()
            .map_err(|_| "embedded backend state lock is poisoned".to_string())?
            .as_ref()
            .cloned()
        {
            return Ok(backend);
        }

        let backend = Arc::new(
            EmbeddedKernelMcpBackend::open_with_engine_and_commit_native(
                &self.data_dir,
                self.engine,
                self.commit_native.clone(),
            )
            .map_err(|error| format!("embedded store is unavailable: {error}"))?,
        );
        let mut opened = self
            .opened
            .lock()
            .map_err(|_| "embedded backend state lock is poisoned".to_string())?;
        let winner = opened.get_or_insert_with(|| Arc::clone(&backend));
        Ok(Arc::clone(winner))
    }
}

impl KernelMcpToolBackend for RetryingEmbeddedKernelMcpBackend {
    fn backend_name(&self) -> &'static str {
        "embedded"
    }

    /// Read from the store once it opens. A store that cannot open bridges
    /// nothing, which is the same answer its `ask` would give.
    fn bridges_languages(&self) -> bool {
        self.opened_backend()
            .map(|backend| backend.bridges_languages())
            .unwrap_or(false)
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        Box::pin(async move {
            let backend = self.opened_backend()?;
            backend.call_tool(name, arguments).await
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn a_permanent_layout_error_never_promises_that_retry_will_fix_it() {
        let data_dir = tempfile::tempdir().expect("temp data dir");
        let store =
            kmp_embedded::store_file_path_for(data_dir.path(), kmp_embedded::StorageEngine::Sqlite);
        std::fs::create_dir_all(store.parent().expect("parent")).expect("store dir");
        std::fs::write(store, b"memory remains on disk").expect("store marker");
        std::fs::write(data_dir.path().join("FORMAT_VERSION"), "3\n").expect("newer format stamp");
        let backend = RetryingEmbeddedKernelMcpBackend::new(data_dir.path(), None);

        let error = backend
            .call_tool(
                "kmp_inspect",
                &json!({"about": "incident:format", "ref": "incident:format"}),
            )
            .await
            .expect_err("a newer layout cannot open");
        assert!(error.message.contains("upgrade the binary"), "{error}");
        assert!(!error.message.contains("temporarily"), "{error}");
        assert!(
            !error.message.contains("next tool call will retry"),
            "{error}"
        );
    }
}
