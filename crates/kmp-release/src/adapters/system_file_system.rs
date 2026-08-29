use std::path::Path;

use crate::domain::release_error::ReleaseError;
use crate::ports::candidate_file_system::CandidateFileSystem;
use crate::ports::release_file_system::ReleaseFileSystem;

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemFileSystem;

impl ReleaseFileSystem for SystemFileSystem {
    fn read_text(&self, path: &Path) -> Result<String, ReleaseError> {
        std::fs::read_to_string(path).map_err(|error| ReleaseError::io("read", path, &error))
    }

    fn write_text(&self, path: &Path, content: &str) -> Result<(), ReleaseError> {
        std::fs::write(path, content).map_err(|error| ReleaseError::io("write", path, &error))
    }
}

impl CandidateFileSystem for SystemFileSystem {
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, ReleaseError> {
        std::fs::read(path).map_err(|error| ReleaseError::io("read", path, &error))
    }

    fn write_bytes(&self, path: &Path, content: &[u8]) -> Result<(), ReleaseError> {
        std::fs::write(path, content).map_err(|error| ReleaseError::io("write", path, &error))
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), ReleaseError> {
        std::fs::create_dir_all(path)
            .map_err(|error| ReleaseError::io("create directory", path, &error))
    }

    fn remove_dir_all_if_present(&self, path: &Path) -> Result<(), ReleaseError> {
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ReleaseError::io("remove directory", path, &error)),
        }
    }

    fn copy_file(&self, source: &Path, destination: &Path) -> Result<(), ReleaseError> {
        std::fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| ReleaseError::io("copy to", destination, &error))
    }

    fn walk_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, ReleaseError> {
        let mut pending = vec![root.to_path_buf()];
        let mut files = Vec::new();
        while let Some(directory) = pending.pop() {
            let entries = std::fs::read_dir(&directory)
                .map_err(|error| ReleaseError::io("list directory", &directory, &error))?;
            for entry in entries {
                let entry = entry.map_err(|error| {
                    ReleaseError::io("read directory entry", &directory, &error)
                })?;
                let path = entry.path();
                let kind = entry
                    .file_type()
                    .map_err(|error| ReleaseError::io("inspect", &path, &error))?;
                if kind.is_dir() {
                    pending.push(path);
                } else if kind.is_file() {
                    files.push(path);
                }
            }
        }
        files.sort();
        Ok(files)
    }

    fn file_size(&self, path: &Path) -> Result<u64, ReleaseError> {
        std::fs::metadata(path)
            .map(|metadata| metadata.len())
            .map_err(|error| ReleaseError::io("inspect", path, &error))
    }

    fn is_executable(&self, path: &Path) -> Result<bool, ReleaseError> {
        let metadata =
            std::fs::metadata(path).map_err(|error| ReleaseError::io("inspect", path, &error))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            Ok(metadata.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            Ok(path.extension().is_some_and(|extension| extension == "exe"))
        }
    }
}
