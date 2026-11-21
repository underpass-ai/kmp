use crate::lifecycle::domain::hand_wired_registration::{
    HandWiredRegistration, KMP_SERVER_ID, RETIRED_SERVER_ID,
};
use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::survey_roots::SurveyRoots;
use crate::lifecycle::ports::installation_catalog::InstallationCatalog;

/// Use case: every registration of KMP written by hand into a host's own
/// configuration — Claude Code's `~/.claude.json` and Codex's
/// `$CODEX_HOME/config.toml` (`~/.codex` when `CODEX_HOME` is unset).
///
/// One survey for both readers: `uninstall` lists what it finds, and
/// `doctor` judges it against the plugins a host reports.
pub struct SurveyHandWiring<'a> {
    installation: &'a dyn InstallationCatalog,
}

impl<'a> SurveyHandWiring<'a> {
    pub fn new(installation: &'a dyn InstallationCatalog) -> Self {
        Self { installation }
    }

    pub fn execute(&self, roots: &SurveyRoots) -> Vec<HandWiredRegistration> {
        let mut found = Vec::new();
        let claude_config = roots.home.join(".claude.json");
        for server_id in [KMP_SERVER_ID, RETIRED_SERVER_ID] {
            if self
                .installation
                .file_mentions(&claude_config, &format!("\"{server_id}\""))
            {
                found.push(HandWiredRegistration::new(
                    Host::Claude,
                    server_id,
                    claude_config.clone(),
                ));
            }
        }
        let codex_config = roots.codex_home.join("config.toml");
        for server_id in [KMP_SERVER_ID, RETIRED_SERVER_ID] {
            if self
                .installation
                .file_mentions(&codex_config, &format!("[mcp_servers.{server_id}]"))
            {
                found.push(HandWiredRegistration::new(
                    Host::Codex,
                    server_id,
                    codex_config.clone(),
                ));
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::lifecycle::adapters::native_installation_catalog::NativeInstallationCatalog;

    fn roots(base: &Path, codex_home: &Path) -> SurveyRoots {
        SurveyRoots {
            home: base.join("home"),
            data_home: base.join("home/.local/share"),
            codex_home: codex_home.to_path_buf(),
            working_dir: base.join("project"),
            path_entries: Vec::new(),
        }
    }

    #[test]
    fn codex_wiring_is_read_where_codex_home_points_not_under_home() {
        let base = tempfile::tempdir().expect("tempdir");
        let codex_home = base.path().join("elsewhere/codex");
        std::fs::create_dir_all(&codex_home).expect("codex home");
        std::fs::write(
            codex_home.join("config.toml"),
            "[mcp_servers.kmp]\ncommand = \"kmp-mcp\"\n[mcp_servers.kernel-memory]\ncommand = \"x\"\n",
        )
        .expect("config");
        // A stale copy under HOME is not what Codex reads, so it is not reported.
        std::fs::create_dir_all(base.path().join("home/.codex")).expect("home codex");
        std::fs::write(
            base.path().join("home/.codex/config.toml"),
            "[mcp_servers.kmp]\n",
        )
        .expect("stale");

        let found = SurveyHandWiring::new(&NativeInstallationCatalog)
            .execute(&roots(base.path(), &codex_home));

        assert_eq!(
            found,
            vec![
                HandWiredRegistration::new(Host::Codex, "kmp", codex_home.join("config.toml")),
                HandWiredRegistration::new(
                    Host::Codex,
                    "kernel-memory",
                    codex_home.join("config.toml")
                ),
            ]
        );
    }

    #[test]
    fn a_plugin_only_machine_has_no_hand_wiring() {
        let base = tempfile::tempdir().expect("tempdir");
        let codex_home = base.path().join("home/.codex");
        std::fs::create_dir_all(&codex_home).expect("codex home");
        std::fs::write(
            codex_home.join("config.toml"),
            "[plugins.\"kmp@underpass\"]\nenabled = true\n[mcp_servers.kmp.env]\nX = \"1\"\n",
        )
        .expect("config");
        std::fs::write(
            base.path().join("home/.claude.json"),
            "{\"enabledPlugins\":{\"kmp@underpass\":true}}",
        )
        .expect("claude");

        assert!(
            SurveyHandWiring::new(&NativeInstallationCatalog)
                .execute(&roots(base.path(), &codex_home))
                .is_empty()
        );
    }
}
