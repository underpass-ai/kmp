use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::lifecycle::adapters::bounded_child::BoundedChild;
use crate::lifecycle::domain::engine_artifact::EngineArtifact;
use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::engine_install_dir::EngineInstallDir;
use crate::lifecycle::domain::engine_proof::EngineProof;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::process_timeout::ProcessTimeout;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::domain::tree_digest::TreeDigest;
use crate::lifecycle::ports::engine_store::EngineStore;

static UNIQUE_PATH: AtomicU64 = AtomicU64::new(0);

/// Filesystem adapter for atomic engine replacement and black-box MCP proof.
#[derive(Clone, Copy, Debug, Default)]
pub struct FilesystemEngineStore;

impl FilesystemEngineStore {
    fn io(path: &Path, error: impl std::fmt::Display) -> LifecycleError {
        LifecycleError::Io {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
    }

    fn unique_path(parent: &Path, label: &str) -> PathBuf {
        let ordinal = UNIQUE_PATH.fetch_add(1, Ordering::Relaxed);
        parent.join(format!(".kmp-{label}-{}-{ordinal}", std::process::id()))
    }

    fn make_executable(path: &Path) -> Result<(), LifecycleError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path)
                .map_err(|error| Self::io(path, error))?
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).map_err(|error| Self::io(path, error))?;
        }
        Ok(())
    }

    fn replace(temp: &Path, destination: &Path) -> Result<(), LifecycleError> {
        #[cfg(not(windows))]
        {
            fs::rename(temp, destination).map_err(|error| Self::io(destination, error))
        }
        #[cfg(windows)]
        {
            let previous = destination.with_extension("kmp-previous.exe");
            if destination.exists() {
                fs::rename(destination, &previous).map_err(|error| Self::io(destination, error))?;
            }
            match fs::rename(temp, destination) {
                Ok(()) => {
                    let _ = fs::remove_file(previous);
                    Ok(())
                }
                Err(error) => {
                    let _ = fs::rename(previous, destination);
                    Err(Self::io(destination, error))
                }
            }
        }
    }

    fn collect_tree(
        root: &Path,
        current: &Path,
        files: &mut Vec<PathBuf>,
    ) -> Result<(), LifecycleError> {
        let mut entries = fs::read_dir(current)
            .map_err(|error| Self::io(current, error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| Self::io(current, error))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap_or(&path);
            // Host-owned lease and runtime files are not marketplace payload.
            // Claude owns `.in_use`; its plugin-local engine is installed only
            // after exact Claude/Codex payload parity has been proved.
            if relative == Path::new(".in_use")
                || relative == Path::new("bin").join("kmp-mcp")
                || relative == Path::new("bin").join("kmp-mcp.exe")
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path).map_err(|error| Self::io(&path, error))?;
            if metadata.is_dir() {
                Self::collect_tree(root, &path, files)?;
            } else {
                files.push(relative.to_path_buf());
            }
        }
        Ok(())
    }

    fn wait_for_proof(child: Child, executable: &Path) -> Result<Output, LifecycleError> {
        let timeout = ProcessTimeout::seconds(15);
        let (output, timed_out) =
            BoundedChild::wait(child, timeout).map_err(|error| Self::io(executable, error))?;
        if timed_out {
            return Err(LifecycleError::SurfaceMismatch(format!(
                "{} did not answer its lifecycle proof within {} seconds: {}",
                executable.display(),
                timeout.duration().as_secs(),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output)
    }
}

impl EngineStore for FilesystemEngineStore {
    fn running_engine(
        &self,
        target: &ReleaseVersion,
    ) -> Result<Option<EngineArtifact>, LifecycleError> {
        if !target.represents_same_release(&ReleaseVersion::current()) {
            return Ok(None);
        }
        let executable = std::env::current_exe().map_err(|error| LifecycleError::Io {
            path: PathBuf::from("<current executable>"),
            detail: error.to_string(),
        })?;
        let bytes = fs::read(&executable).map_err(|error| Self::io(&executable, error))?;
        Ok(Some(EngineArtifact::verified(target.clone(), bytes)))
    }

    fn install(
        &self,
        artifact: &EngineArtifact,
        destination: &EngineInstallDir,
    ) -> Result<EngineExecutable, LifecycleError> {
        fs::create_dir_all(destination.as_path())
            .map_err(|error| Self::io(destination.as_path(), error))?;
        let executable = destination.executable();
        let temp = Self::unique_path(destination.as_path(), "engine");
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp)
                .map_err(|error| Self::io(&temp, error))?;
            file.write_all(artifact.bytes())
                .map_err(|error| Self::io(&temp, error))?;
            file.sync_all().map_err(|error| Self::io(&temp, error))?;
            Self::make_executable(&temp)?;
            Self::replace(&temp, &executable)?;
            Ok(EngineExecutable::installed_at(executable.clone()))
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }

    fn stage_and_prove(
        &self,
        artifact: &EngineArtifact,
        target: &ReleaseVersion,
    ) -> Result<EngineProof, LifecycleError> {
        let scratch = Self::unique_path(&std::env::temp_dir(), "lifecycle-stage");
        let result = (|| {
            let destination = EngineInstallDir::new(&scratch)?;
            let executable = self.install(artifact, &destination)?;
            self.prove(&executable, target)
        })();
        let _ = fs::remove_dir_all(&scratch);
        result
    }

    fn prove(
        &self,
        executable: &EngineExecutable,
        target: &ReleaseVersion,
    ) -> Result<EngineProof, LifecycleError> {
        let version_child = Command::new(executable.as_path())
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| Self::io(executable.as_path(), error))?;
        let version_output = Self::wait_for_proof(version_child, executable.as_path())?;
        let version_text = String::from_utf8_lossy(&version_output.stdout);
        let expected_prefix = format!("kmp-mcp {}", target.engine_version());
        if !version_output.status.success() || !version_text.starts_with(&expected_prefix) {
            return Err(LifecycleError::SurfaceMismatch(format!(
                "{} did not report {}: {}",
                executable.as_path().display(),
                target,
                version_text.trim()
            )));
        }

        let scratch = Self::unique_path(&std::env::temp_dir(), "lifecycle-probe");
        fs::create_dir_all(&scratch).map_err(|error| Self::io(&scratch, error))?;
        let result = (|| {
            let mut child = Command::new(executable.as_path())
                .env_remove("KMP_MCP_BIN")
                .env("KMP_MCP_BACKEND", "embedded")
                .env("KMP_MCP_DATA_DIR", &scratch)
                .env("KMP_VIEWER_ADDR", "off")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| Self::io(executable.as_path(), error))?;
            let request = concat!(
                "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",",
                "\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},",
                "\"clientInfo\":{\"name\":\"kmp-lifecycle\",\"version\":\"1\"}}}\n",
                "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n"
            );
            child
                .stdin
                .as_mut()
                .ok_or_else(|| {
                    LifecycleError::SurfaceMismatch("probe stdin is unavailable".to_string())
                })?
                .write_all(request.as_bytes())
                .map_err(|error| Self::io(executable.as_path(), error))?;
            drop(child.stdin.take());
            Self::wait_for_proof(child, executable.as_path())
        })();
        let _ = fs::remove_dir_all(&scratch);
        let output = result?;
        if !output.status.success() {
            return Err(LifecycleError::SurfaceMismatch(format!(
                "{} exited during MCP proof: {}",
                executable.as_path().display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let mut initialized_version = None;
        let mut tools = BTreeSet::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            match message["id"].as_u64() {
                Some(1) => {
                    initialized_version = message["result"]["serverInfo"]["version"]
                        .as_str()
                        .map(str::to_string);
                }
                Some(2) => {
                    if let Some(list) = message["result"]["tools"].as_array() {
                        tools.extend(
                            list.iter()
                                .filter_map(|tool| tool["name"].as_str().map(str::to_string)),
                        );
                    }
                }
                _ => {}
            }
        }
        let expected = crate::tool_names().into_iter().collect::<BTreeSet<_>>();
        if initialized_version.as_deref() != Some(target.engine_version()) || tools != expected {
            return Err(LifecycleError::SurfaceMismatch(format!(
                "{} failed the exact lifecycle proof: version={initialized_version:?}, missing={:?}, unexpected={:?}",
                executable.as_path().display(),
                expected.difference(&tools).collect::<Vec<_>>(),
                tools.difference(&expected).collect::<Vec<_>>()
            )));
        }
        Ok(EngineProof::proven(
            executable.clone(),
            target.clone(),
            tools.into_iter().collect(),
        ))
    }

    fn digest_tree(&self, root: &PluginRoot) -> Result<TreeDigest, LifecycleError> {
        let mut files = Vec::new();
        Self::collect_tree(root.as_path(), root.as_path(), &mut files)?;
        files.sort();
        let mut digest = Sha256::new();
        for relative in files {
            let path = root.as_path().join(&relative);
            digest.update(relative.to_string_lossy().as_bytes());
            digest.update(b"\0");
            let metadata = fs::symlink_metadata(&path).map_err(|error| Self::io(&path, error))?;
            if metadata.file_type().is_symlink() {
                digest.update(
                    fs::read_link(&path)
                        .map_err(|error| Self::io(&path, error))?
                        .to_string_lossy()
                        .as_bytes(),
                );
            } else {
                digest.update(fs::read(&path).map_err(|error| Self::io(&path, error))?);
            }
            digest.update(b"\0");
        }
        Ok(TreeDigest::sha256(format!("{:x}", digest.finalize())))
    }
}
