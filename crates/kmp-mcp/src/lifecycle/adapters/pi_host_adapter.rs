use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::lifecycle::adapters::mappers::pi_runtime_status_mapper::PiRuntimeStatusMapper;
use crate::lifecycle::adapters::native_skill_mirror::NativeSkillMirror;
use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::pi_agent_home::PiAgentHome;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::process_executor::ProcessExecutor;

/// Adapter for the Pi coding agent host.
///
/// Pi has no native MCP and no plugin manager. KMP reaches it through the
/// `underpass-pi` Pi package, whose presence in `settings.json` is the
/// registration evidence; installing that package belongs to
/// `underpass setup`, never to this process. What a convergence does here is
/// mirror the plugin's skills where Pi scans, and refuse to call Pi converged
/// while the package that carries the connection is absent or disabled.
pub struct PiHostAdapter<'a> {
    processes: &'a dyn ProcessExecutor,
    home: PiAgentHome,
}

impl<'a> PiHostAdapter<'a> {
    /// The home Pi itself resolves (see [`PiAgentHome::resolve`]).
    pub fn new(processes: &'a dyn ProcessExecutor) -> Result<Self, LifecycleError> {
        let agent_dir = std::env::var_os("PI_CODING_AGENT_DIR");
        let user_home = std::env::var_os("HOME").map(PathBuf::from);
        let working_dir = std::env::current_dir().ok();
        let home = PiAgentHome::resolve(
            agent_dir.as_deref(),
            user_home.as_deref(),
            working_dir.as_deref(),
        )?;
        Ok(Self { processes, home })
    }

    pub fn with_home(processes: &'a dyn ProcessExecutor, home: PiAgentHome) -> Self {
        Self { processes, home }
    }

    /// What `settings.json` says about the package that carries KMP into Pi.
    pub fn runtime_status(&self) -> HostRuntimeStatus {
        match fs::read_to_string(self.home.settings_file()) {
            Ok(settings) => PiRuntimeStatusMapper::map(Some(&settings)),
            Err(error) if error.kind() == ErrorKind::NotFound => PiRuntimeStatusMapper::map(None),
            Err(error) => {
                HostRuntimeStatus::Failed(format!("Pi settings.json could not be read: {error}"))
            }
        }
    }

    /// Which plugin skills Pi can already discover.
    pub fn installed_skills(&self) -> Vec<String> {
        self.home
            .installed_skills(&NativeSkillMirror::skill_names())
    }

    /// The installation Pi declares: the `underpass-pi` package plus the
    /// discoverable skills, rooted at the Pi agent home that owns both. None
    /// when Pi holds no KMP surface at all. Only an enabled package makes it
    /// a consumer of the shared engine.
    pub fn installation(
        &self,
        version: &ReleaseVersion,
    ) -> Result<Option<HostInstallation>, LifecycleError> {
        let status = self.runtime_status();
        let declared = matches!(
            status,
            HostRuntimeStatus::Registered | HostRuntimeStatus::Disabled
        );
        if !declared && self.installed_skills().is_empty() {
            return Ok(None);
        }
        Ok(Some(HostInstallation::discovered(
            Host::Pi,
            version.clone(),
            PluginRoot::new(self.home.as_path())?,
            status == HostRuntimeStatus::Registered,
        )))
    }

    /// Mirror the plugin's skills when a plugin tree is available, then
    /// require the `underpass-pi` package. Without it the skills are in place
    /// but Pi still has no KMP tools, and the failure says how to fix that.
    pub fn provision(
        &self,
        version: &ReleaseVersion,
        plugin_root: Option<&Path>,
    ) -> Result<HostInstallation, LifecycleError> {
        self.converge(version, plugin_root)
    }

    /// Pi holds no versioned artifact: refreshing is the same convergence as
    /// provisioning, answered at the target release (#868).
    pub fn refresh(
        &self,
        version: &ReleaseVersion,
        plugin_root: Option<&Path>,
    ) -> Result<HostInstallation, LifecycleError> {
        self.converge(version, plugin_root)
    }

    fn converge(
        &self,
        version: &ReleaseVersion,
        plugin_root: Option<&Path>,
    ) -> Result<HostInstallation, LifecycleError> {
        if let Some(plugin_root) = plugin_root {
            NativeSkillMirror::for_host(Host::Pi).mirror(plugin_root, &self.home.skills_dir())?;
        }
        let status = self.runtime_status();
        if status != HostRuntimeStatus::Registered {
            return Err(LifecycleError::HostNotInstalled(Self::unregistered(
                &status,
            )));
        }
        self.installation(version)?.ok_or_else(|| {
            LifecycleError::InvalidHostResponse(
                "Pi reported the underpass-pi package but no installation".to_string(),
            )
        })
    }

    fn unregistered(status: &HostRuntimeStatus) -> String {
        let reason = match status {
            HostRuntimeStatus::Disabled => {
                "the underpass-pi package excludes its KMP extension in Pi settings.json"
                    .to_string()
            }
            HostRuntimeStatus::Failed(detail) => detail.clone(),
            _ => "Pi settings.json lists no underpass-pi package".to_string(),
        };
        format!(
            "{reason}. Pi has no native MCP: KMP reaches it only through the underpass-pi \
             package. Run `underpass setup` to install it, then rerun `kmp-mcp setup --pi`"
        )
    }

    /// Pi's KMP extension spawns `kmp-mcp` by its PATH name, as Codex and
    /// Hermes do, so the runtime engine is whatever PATH resolves.
    pub fn runtime_engine(&self) -> Result<EngineExecutable, LifecycleError> {
        self.processes
            .resolve("kmp-mcp")
            .map(EngineExecutable::installed_at)
            .ok_or_else(|| {
                LifecycleError::HostNotInstalled(
                    "Pi's underpass-pi package runs `kmp-mcp`, but no executable resolves on PATH"
                        .to_string(),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::lifecycle::adapters::tests_support::FakeProcessExecutor;

    const REGISTERED: &str = r#"{"packages":["../../Documents/ai/underpass-pi"]}"#;

    fn version() -> ReleaseVersion {
        ReleaseVersion::parse("0.18.9").expect("version")
    }

    fn pi_home(settings: Option<&str>, skills: &[&str]) -> (tempfile::TempDir, PiAgentHome) {
        let root = tempfile::tempdir().expect("tempdir");
        let home = PiAgentHome::new(root.path().join(".pi/agent")).expect("home");
        fs::create_dir_all(home.as_path()).expect("pi home");
        if let Some(settings) = settings {
            fs::write(home.settings_file(), settings).expect("settings");
        }
        for name in skills {
            let dir = home.skill(name);
            fs::create_dir_all(&dir).expect("skill dir");
            fs::write(dir.join("SKILL.md"), "---\nname: x\n---").expect("skill md");
        }
        (root, home)
    }

    fn plugin_with(names: &[&str]) -> tempfile::TempDir {
        let plugin = tempfile::tempdir().expect("plugin root");
        for name in names {
            let dir = plugin.path().join("skills").join(name);
            fs::create_dir_all(&dir).expect("plugin skill dir");
            fs::write(dir.join("SKILL.md"), "---\nname: x\n---").expect("plugin skill md");
        }
        plugin
    }

    #[test]
    fn an_unconfigured_pi_declares_no_installation() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, home) = pi_home(Some(r#"{"packages":["npm:pi-mcp-adapter"]}"#), &[]);
        let adapter = PiHostAdapter::with_home(&processes, home);

        assert_eq!(adapter.runtime_status(), HostRuntimeStatus::Missing);
        assert!(
            adapter
                .installation(&version())
                .expect("installation")
                .is_none()
        );
        assert!(processes.is_exhausted());
    }

    #[test]
    fn a_registered_underpass_package_is_an_installation() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, home) = pi_home(Some(REGISTERED), &["kmp-doctor", "kmp-guide"]);
        let expected_root = home.as_path().to_path_buf();
        let adapter = PiHostAdapter::with_home(&processes, home);

        let installation = adapter
            .installation(&version())
            .expect("installation")
            .expect("Pi holds a KMP surface");
        assert_eq!(installation.host(), Host::Pi);
        assert!(installation.is_enabled());
        assert_eq!(installation.root().as_path(), expected_root);
        assert_eq!(
            adapter.installed_skills(),
            vec!["kmp-doctor".to_string(), "kmp-guide".to_string()]
        );
    }

    #[test]
    fn a_disabled_kmp_extension_is_an_installation_that_does_not_converge() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, home) = pi_home(
            Some(
                r#"{"packages":[{"source":"../underpass-pi","extensions":["!src/adapters/inbound/pi/entry/kmp.ts"]}]}"#,
            ),
            &[],
        );
        let adapter = PiHostAdapter::with_home(&processes, home);

        let installation = adapter
            .installation(&version())
            .expect("installation")
            .expect("the package is declared");
        assert!(!installation.is_enabled());
    }

    #[test]
    fn mirroring_brings_the_plugin_skills_where_pi_scans() {
        let plugin = plugin_with(&["kmp-doctor", "kmp-guide", "kmp-memory"]);
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, home) = pi_home(Some(REGISTERED), &[]);
        let adapter = PiHostAdapter::with_home(&processes, home);

        let installation = adapter
            .provision(&version(), Some(plugin.path()))
            .expect("provision");

        assert!(installation.is_enabled());
        assert_eq!(
            adapter.installed_skills(),
            vec![
                "kmp-doctor".to_string(),
                "kmp-guide".to_string(),
                "kmp-memory".to_string()
            ]
        );
    }

    #[test]
    fn provision_without_registration_points_to_underpass_setup() {
        let plugin = plugin_with(&["kmp-doctor"]);
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, home) = pi_home(None, &[]);
        let adapter = PiHostAdapter::with_home(&processes, home);

        let error = adapter
            .provision(&version(), Some(plugin.path()))
            .expect_err("no underpass-pi package");

        let message = error.to_string();
        assert!(message.contains("underpass setup"), "{message}");
        assert!(message.contains("no underpass-pi package"), "{message}");
        // The skills still land, so the rerun only has the package left to do.
        assert_eq!(adapter.installed_skills(), vec!["kmp-doctor".to_string()]);
    }

    #[test]
    fn a_disabled_extension_is_named_when_convergence_refuses() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, home) = pi_home(
            Some(
                r#"{"packages":[{"source":"../underpass-pi","extensions":["!src/adapters/inbound/pi/entry/kmp.ts"]}]}"#,
            ),
            &[],
        );
        let adapter = PiHostAdapter::with_home(&processes, home);

        let message = adapter
            .refresh(&version(), None)
            .expect_err("KMP extension excluded")
            .to_string();
        assert!(message.contains("excludes its KMP extension"), "{message}");
        assert!(message.contains("underpass setup"), "{message}");
    }

    #[test]
    fn unreadable_settings_are_a_failure_with_their_reason() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, home) = pi_home(Some("{not json"), &[]);
        let adapter = PiHostAdapter::with_home(&processes, home);

        assert!(matches!(
            adapter.runtime_status(),
            HostRuntimeStatus::Failed(detail) if detail.contains("not valid JSON")
        ));
        let message = adapter
            .provision(&version(), None)
            .expect_err("broken settings")
            .to_string();
        assert!(message.contains("not valid JSON"), "{message}");
    }

    #[test]
    fn a_settings_path_that_cannot_be_read_is_a_failure() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, home) = pi_home(None, &[]);
        fs::create_dir_all(home.settings_file()).expect("a directory where the file goes");
        let adapter = PiHostAdapter::with_home(&processes, home);

        assert!(matches!(
            adapter.runtime_status(),
            HostRuntimeStatus::Failed(detail) if detail.contains("could not be read")
        ));
    }

    #[test]
    fn refresh_answers_the_target_release() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, home) = pi_home(Some(REGISTERED), &[]);
        let adapter = PiHostAdapter::with_home(&processes, home);
        let target = ReleaseVersion::parse("9.9.9").expect("target");

        let refreshed = adapter.refresh(&target, None).expect("refresh");
        assert_eq!(refreshed.version(), &target);
    }

    #[test]
    fn the_runtime_engine_is_what_path_resolves() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, home) = pi_home(None, &[]);
        let adapter = PiHostAdapter::with_home(&processes, home);

        let engine = adapter.runtime_engine().expect("engine");
        assert!(engine.as_path().ends_with("kmp-mcp"));
    }
}
