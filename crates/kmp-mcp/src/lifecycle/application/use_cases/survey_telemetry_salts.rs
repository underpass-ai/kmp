//! Fingerprint salts this machine keeps for remote kernels' call logs,
//! beside the agent directories of each gRPC endpoint. An embedded store's
//! salt lives inside the store and goes with it; these would be left behind.

use crate::lifecycle::domain::piece::Piece;
use crate::lifecycle::domain::piece_kind::PieceKind;
use crate::lifecycle::domain::survey_roots::SurveyRoots;
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;

pub struct SurveyTelemetrySalts<'a> {
    installation: &'a dyn InstallationCatalog,
}

impl<'a> SurveyTelemetrySalts<'a> {
    pub fn new(installation: &'a dyn InstallationCatalog) -> Self {
        Self { installation }
    }

    pub fn execute(&self, roots: &SurveyRoots) -> Vec<Piece> {
        let suffix = format!(".{}", kmp_observability::TELEMETRY_SALT_FILE);
        self.installation
            .files_in(&roots.data_home.join("agent-users"))
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(&suffix))
            })
            .map(|path| Piece {
                kind: PieceKind::HostFiles,
                detail:
                    "fingerprint salt of a remote kernel's call log — never leaves this machine"
                        .to_string(),
                path,
                bundled_events: None,
                ours_to_remove: true,
                held_by: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::SurveyTelemetrySalts;
    use crate::lifecycle::adapters::native_installation_catalog::NativeInstallationCatalog;
    use crate::lifecycle::domain::piece_kind::PieceKind;
    use crate::lifecycle::domain::survey_roots::SurveyRoots;

    #[test]
    fn remote_salts_are_listed_for_removal_and_agent_directories_are_not() {
        let base = tempfile::tempdir().expect("dir");
        let roots = SurveyRoots {
            home: base.path().join("home"),
            data_home: base.path().join("data"),
            working_dir: base.path().join("project"),
            path_entries: Vec::new(),
        };
        let agents = roots.data_home.join("agent-users");
        std::fs::create_dir_all(&agents).expect("agents");
        std::fs::write(agents.join("abc.sqlite3"), b"agents").expect("agents");
        std::fs::write(agents.join("abc.telemetry-salt"), [0u8; 32]).expect("salt");

        let pieces = SurveyTelemetrySalts::new(&NativeInstallationCatalog).execute(&roots);
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].kind, PieceKind::HostFiles);
        assert!(pieces[0].ours_to_remove);
        assert_eq!(pieces[0].path, agents.join("abc.telemetry-salt"));

        let none = SurveyTelemetrySalts::new(&NativeInstallationCatalog).execute(&SurveyRoots {
            data_home: Path::new("/nonexistent/kmp").to_path_buf(),
            ..roots
        });
        assert!(none.is_empty());
    }
}
