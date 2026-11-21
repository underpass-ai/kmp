//! The file a store leaves beside itself once it holds memory.
//!
//! A lost database is refused only when the directory shows it held memory
//! before. The telemetry salt was that evidence, but only a served wake or
//! ask writes it: a store filled by `kmp-mcp demo`, `kmp-mcp import` or a
//! project rebuild had none, so deleting its database restarted it empty
//! without a word. This file is written by the store itself, on the first
//! commit that appends an event, however that event arrived.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use kmp_domain::PortError;

/// Beside `FORMAT_VERSION`, outside `store/`: deleting the database (or the
/// whole `store/` directory) leaves it behind as the evidence.
pub(crate) const MEMORY_PROOF_FILE: &str = "held-memory";

const MEMORY_PROOF_TEXT: &str = "This KMP store has held memory. If store/kernel.sqlite3 is \
missing or empty, KMP refuses to start an empty store over it.\n";

/// Where a data directory's proof lives.
pub(crate) fn memory_proof_path(data_dir: &Path) -> PathBuf {
    data_dir.join(MEMORY_PROOF_FILE)
}

/// One store's proof: written once, then only checked in memory.
#[derive(Debug)]
pub(crate) struct MemoryProof {
    path: PathBuf,
    recorded: AtomicBool,
}

impl MemoryProof {
    pub(crate) fn for_data_dir(data_dir: &Path) -> Self {
        let path = memory_proof_path(data_dir);
        let recorded = AtomicBool::new(path.is_file());
        Self { path, recorded }
    }

    pub(crate) fn is_recorded(&self) -> bool {
        self.recorded.load(Ordering::Acquire)
    }

    /// Called after a commit that appended events. Idempotent; a failure
    /// is returned so the write that needed the proof says it is missing.
    pub(crate) fn record(&self) -> Result<(), PortError> {
        if self.is_recorded() {
            return Ok(());
        }
        std::fs::write(&self.path, MEMORY_PROOF_TEXT).map_err(|error| {
            PortError::Unavailable(format!(
                "embedded store committed, but could not write its proof of memory `{}`: \
                 {error}; a later loss of the database would not be detected",
                self.path.display()
            ))
        })?;
        self.recorded.store(true, Ordering::Release);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_proof_is_written_once_beside_the_store() {
        let dir = tempfile::tempdir().expect("dir");
        let proof = MemoryProof::for_data_dir(dir.path());
        assert!(!proof.is_recorded());
        proof.record().expect("recorded");
        assert!(proof.is_recorded());
        assert!(memory_proof_path(dir.path()).is_file());
        proof.record().expect("second record is a no-op");
        assert!(MemoryProof::for_data_dir(dir.path()).is_recorded());
    }

    #[test]
    fn a_proof_that_cannot_be_written_says_so() {
        let dir = tempfile::tempdir().expect("dir");
        let proof = MemoryProof::for_data_dir(&dir.path().join("missing/dir"));
        let error = proof.record().expect_err("no directory").to_string();
        assert!(error.contains("proof of memory"), "{error}");
        assert!(!proof.is_recorded());
    }
}
