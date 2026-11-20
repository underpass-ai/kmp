use std::path::Path;

use crate::lifecycle::adapters::native_process_liveness::NativeProcessLiveness;
use crate::lifecycle::ports::process_liveness::ProcessLiveness;

/// Who is still reading one cached plugin version, as far as this platform
/// can tell.
///
/// A start collects the versions an earlier update superseded, on the
/// premise that no session can still be reading them (#521). That premise
/// is checked here instead of assumed (S5): a version is in use when a live
/// process holds one of Claude Code's `.in_use` markers in it, or — where
/// procfs exists — when a running process executes from inside it or was
/// started by its launcher, which exports the version directory as
/// `KMP_PLUGIN_ROOT`.
///
/// Codex writes no marker and runs the shared engine by its PATH name, so a
/// Codex session that captured its skills from an older version leaves no
/// trace here; that case is still covered only by the deferral itself.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeReleaseReaders;

impl NativeReleaseReaders {
    /// A live process reading `version_dir`, other than this one.
    pub fn reader_of(&self, version_dir: &Path) -> Option<u32> {
        Self::marker_holder(version_dir).or_else(|| Self::running_from(version_dir))
    }

    fn marker_holder(version_dir: &Path) -> Option<u32> {
        let mut live = std::fs::read_dir(version_dir.join(".in_use"))
            .ok()?
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
            .filter(|pid| *pid != std::process::id() && NativeProcessLiveness.is_running(*pid))
            .collect::<Vec<_>>();
        live.sort_unstable();
        live.first().copied()
    }

    #[cfg(target_os = "linux")]
    fn running_from(version_dir: &Path) -> Option<u32> {
        let version_dir = std::fs::canonicalize(version_dir).ok()?;
        let own = std::process::id();
        std::fs::read_dir("/proc")
            .ok()?
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
            .filter(|pid| *pid != own)
            .find(|pid| {
                let process = std::path::PathBuf::from("/proc").join(pid.to_string());
                let executes_from = std::fs::read_link(process.join("exe"))
                    .is_ok_and(|exe| exe.starts_with(&version_dir));
                executes_from || Self::launched_from(&process, &version_dir)
            })
    }

    #[cfg(not(target_os = "linux"))]
    fn running_from(_version_dir: &Path) -> Option<u32> {
        None
    }

    #[cfg(target_os = "linux")]
    fn launched_from(process: &Path, version_dir: &Path) -> bool {
        let Ok(environment) = std::fs::read(process.join("environ")) else {
            return false;
        };
        environment
            .split(|byte| *byte == 0)
            .filter_map(|entry| entry.strip_prefix(b"KMP_PLUGIN_ROOT="))
            .filter_map(|root| std::str::from_utf8(root).ok())
            .any(|root| std::fs::canonicalize(root).is_ok_and(|root| root.starts_with(version_dir)))
    }
}

#[cfg(test)]
mod tests {
    use super::NativeReleaseReaders;

    #[cfg(unix)]
    #[test]
    fn a_version_with_a_live_marker_is_in_use_and_a_dead_one_is_not() {
        let base = tempfile::tempdir().expect("temp");
        let live = base.path().join("0.1.0");
        let dead = base.path().join("0.2.0");
        let parent = std::os::unix::process::parent_id();
        for (version, pid) in [(&live, parent), (&dead, u32::MAX - 1)] {
            std::fs::create_dir_all(version.join(".in_use")).expect("markers");
            std::fs::write(version.join(".in_use").join(pid.to_string()), b"{}").expect("marker");
        }

        assert_eq!(NativeReleaseReaders.reader_of(&live), Some(parent));
        assert_eq!(NativeReleaseReaders.reader_of(&dead), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_started_from_a_version_launcher_holds_it() {
        let base = tempfile::tempdir().expect("temp");
        let version = base.path().join("0.3.0");
        std::fs::create_dir_all(&version).expect("version");
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .env("KMP_PLUGIN_ROOT", &version)
            .spawn()
            .expect("a reader");

        let reader = NativeReleaseReaders.reader_of(&version);
        child.kill().expect("stop the reader");
        let _ = child.wait();

        assert_eq!(reader, Some(child.id()));
    }
}
