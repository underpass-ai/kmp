use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use kmp_proto_mapping::v1beta1::LexicalBridge;
use sha2::{Digest, Sha256};

use crate::lifecycle::domain::bridge_install_dir::BridgeInstallDir;
use crate::lifecycle::domain::lexical_bridge_artifact::LexicalBridgeArtifact;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::ports::bridge_store::BridgeStore;

static UNIQUE_PATH: AtomicU64 = AtomicU64::new(0);

/// Filesystem adapter for the machine's lexical-bridge table.
#[derive(Clone, Copy, Debug, Default)]
pub struct FilesystemBridgeStore;

impl FilesystemBridgeStore {
    fn io(path: &Path, error: impl std::fmt::Display) -> LifecycleError {
        LifecycleError::Io {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
    }

    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
}

impl BridgeStore for FilesystemBridgeStore {
    fn installed_digest(&self, destination: &BridgeInstallDir) -> Option<String> {
        fs::read(destination.table())
            .ok()
            .map(|bytes| Self::digest(&bytes))
    }

    fn read(&self, path: &Path) -> Result<LexicalBridgeArtifact, LifecycleError> {
        let bytes = fs::read(path).map_err(|error| Self::io(path, error))?;
        let sha256 = Self::digest(&bytes);
        Ok(LexicalBridgeArtifact::verified(
            bytes,
            sha256,
            path.display().to_string(),
        ))
    }

    fn install(
        &self,
        artifact: &LexicalBridgeArtifact,
        destination: &BridgeInstallDir,
    ) -> Result<PathBuf, LifecycleError> {
        artifact.require_content()?;
        LexicalBridge::from_bytes(artifact.bytes()).map_err(|reason| {
            LifecycleError::SurfaceMismatch(format!(
                "lexical bridge table from {} is not a table the kernel can read: {reason}",
                artifact.source()
            ))
        })?;

        let directory = destination.as_path();
        fs::create_dir_all(directory).map_err(|error| Self::io(directory, error))?;
        let table = destination.table();
        let ordinal = UNIQUE_PATH.fetch_add(1, Ordering::Relaxed);
        let temporary = directory.join(format!(
            ".kmp-lexical-bridge-{}-{ordinal}",
            std::process::id()
        ));
        fs::write(&temporary, artifact.bytes()).map_err(|error| Self::io(&temporary, error))?;
        match fs::rename(&temporary, &table) {
            Ok(()) => Ok(table),
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(Self::io(&table, error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn judged_table() -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../kmp-testkit/judged/lexical-bridge.kmpb"),
        )
        .expect("the judged fixture is committed")
    }

    fn destination(root: &Path) -> BridgeInstallDir {
        BridgeInstallDir::new(root.join("kmp")).expect("tempdirs are absolute")
    }

    #[test]
    fn a_table_is_installed_and_reads_back_with_the_same_digest() {
        let home = tempfile::tempdir().expect("temp");
        let destination = destination(home.path());
        let bytes = judged_table();
        let sha256 = FilesystemBridgeStore::digest(&bytes);
        let artifact = LexicalBridgeArtifact::verified(
            bytes,
            sha256.clone(),
            "the judged fixture".to_string(),
        );

        let installed = FilesystemBridgeStore
            .install(&artifact, &destination)
            .expect("the fixture is a real table");

        assert_eq!(installed, destination.table());
        assert_eq!(
            FilesystemBridgeStore.installed_digest(&destination),
            Some(sha256)
        );
    }

    /// A store that looks equipped and bridges nothing is worse than one that
    /// admits it has no table.
    #[test]
    fn bytes_the_kernel_could_not_read_are_refused_before_anything_is_written() {
        let home = tempfile::tempdir().expect("temp");
        let destination = destination(home.path());
        let artifact = LexicalBridgeArtifact::verified(
            b"not a table".to_vec(),
            "whatever".to_string(),
            "a release asset".to_string(),
        );

        let refused = FilesystemBridgeStore.install(&artifact, &destination);

        assert!(refused.is_err());
        assert!(!destination.table().exists());
        assert_eq!(FilesystemBridgeStore.installed_digest(&destination), None);
    }

    #[test]
    fn an_empty_table_is_refused_as_a_publishing_accident() {
        let home = tempfile::tempdir().expect("temp");
        let destination = destination(home.path());
        let artifact = LexicalBridgeArtifact::verified(
            Vec::new(),
            "whatever".to_string(),
            "a release asset".to_string(),
        );

        assert!(
            FilesystemBridgeStore
                .install(&artifact, &destination)
                .is_err()
        );
    }

    #[test]
    fn a_table_an_operator_built_is_read_with_its_digest() {
        let home = tempfile::tempdir().expect("temp");
        let path = home.path().join("built.kmpb");
        let bytes = judged_table();
        fs::write(&path, &bytes).expect("fixture");

        let artifact = FilesystemBridgeStore.read(&path).expect("readable");

        assert_eq!(artifact.sha256(), FilesystemBridgeStore::digest(&bytes));
        assert_eq!(artifact.source(), path.display().to_string());
    }

    #[test]
    fn a_machine_with_no_table_has_no_digest() {
        let home = tempfile::tempdir().expect("temp");

        assert_eq!(
            FilesystemBridgeStore.installed_digest(&destination(home.path())),
            None
        );
    }
}
