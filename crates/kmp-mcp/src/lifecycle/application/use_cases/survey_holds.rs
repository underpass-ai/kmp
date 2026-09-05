use std::path::Path;

use crate::lifecycle::domain::piece_hold::PieceHold;
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;
use crate::lifecycle::ports::process_liveness::ProcessLiveness;

/// Use case: whether a host is using one installed plugin version right now.
///
/// Claude Code records the sessions reading a plugin version as files under
/// `.in_use`, named by process id. KMP does not write them — it already
/// knows to leave them out of payload parity — and reading them is what lets
/// a dry run answer the question that decides everything else: is this copy
/// idle, or is it the one a live session is still being served from?
///
/// The marker alone does not settle it. A host that crashed leaves its file
/// behind, and treating that as a hold would refuse a removal forever, so
/// every id found is put to the platform before it counts.
pub struct SurveyHolds<'a> {
    installation: &'a dyn InstallationCatalog,
    liveness: &'a dyn ProcessLiveness,
}

impl<'a> SurveyHolds<'a> {
    pub fn new(
        installation: &'a dyn InstallationCatalog,
        liveness: &'a dyn ProcessLiveness,
    ) -> Self {
        Self {
            installation,
            liveness,
        }
    }

    /// The live hold on this plugin version, if a host has one.
    ///
    /// The lowest live id wins when several sessions share a version. Which
    /// one is named matters less than naming one that is really there: the
    /// reader restarts hosts until the line goes away, and a stale id would
    /// send them looking for a window that no longer exists.
    pub fn execute(&self, host: &str, version_directory: &Path) -> Option<PieceHold> {
        let markers = version_directory.join(".in_use");
        if !self.installation.is_directory(&markers) {
            return None;
        }
        let mut live: Vec<u32> = self
            .installation
            .entry_names(&markers)
            .iter()
            .filter_map(|name| name.parse::<u32>().ok())
            .filter(|pid| self.liveness.is_running(*pid))
            .collect();
        live.sort_unstable();
        live.first().map(|pid| PieceHold::new(host, *pid))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::SurveyHolds;
    use crate::lifecycle::adapters::native_installation_catalog::NativeInstallationCatalog;
    use crate::lifecycle::ports::process_liveness::ProcessLiveness;

    struct Running(Vec<u32>);

    impl ProcessLiveness for Running {
        fn is_running(&self, pid: u32) -> bool {
            self.0.contains(&pid)
        }
    }

    fn version_with_markers(pids: &[&str]) -> (tempfile::TempDir, PathBuf) {
        let base = tempfile::tempdir().expect("temp dir");
        let version = base.path().join("0.11.0");
        std::fs::create_dir_all(version.join(".in_use")).expect("marker dir");
        for pid in pids {
            std::fs::write(version.join(".in_use").join(pid), b"{}").expect("marker");
        }
        (base, version)
    }

    #[test]
    fn a_version_a_live_host_is_reading_is_held_by_it() {
        let (_base, version) = version_with_markers(&["868043"]);

        let hold = SurveyHolds::new(&NativeInstallationCatalog, &Running(vec![868_043]))
            .execute("claude", &version);

        assert_eq!(
            hold,
            Some(crate::lifecycle::domain::piece_hold::PieceHold::new(
                "claude", 868_043
            ))
        );
    }

    #[test]
    fn a_marker_left_by_a_host_that_died_is_not_a_hold() {
        // Otherwise the file outlives the crash and nothing is ever
        // removable again.
        let (_base, version) = version_with_markers(&["868043"]);

        let hold = SurveyHolds::new(&NativeInstallationCatalog, &Running(Vec::new()))
            .execute("claude", &version);

        assert_eq!(hold, None);
    }

    #[test]
    fn the_live_id_is_named_even_when_a_dead_one_sorts_before_it() {
        let (_base, version) = version_with_markers(&["100", "868043"]);

        let hold = SurveyHolds::new(&NativeInstallationCatalog, &Running(vec![868_043]))
            .execute("claude", &version);

        assert_eq!(hold.map(|hold| hold.pid()), Some(868_043));
    }

    #[test]
    fn a_version_no_host_ever_opened_has_no_marker_directory_and_no_hold() {
        let base = tempfile::tempdir().expect("temp dir");
        let version = base.path().join("0.12.1");
        std::fs::create_dir_all(&version).expect("version dir");

        let hold = SurveyHolds::new(&NativeInstallationCatalog, &Running(vec![1]))
            .execute("claude", &version);

        assert_eq!(hold, None);
    }

    #[test]
    fn a_marker_that_is_not_a_process_id_is_ignored_rather_than_guessed_at() {
        let (_base, version) = version_with_markers(&["not-a-pid"]);

        let hold = SurveyHolds::new(&NativeInstallationCatalog, &Running(vec![1]))
            .execute("claude", Path::new(&version));

        assert_eq!(hold, None);
    }
}
