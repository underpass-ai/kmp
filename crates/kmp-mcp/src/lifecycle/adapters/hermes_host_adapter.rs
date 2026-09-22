use std::fs;
use std::path::{Path, PathBuf};

use crate::lifecycle::adapters::mappers::hermes_runtime_status_mapper::HermesRuntimeStatusMapper;
use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::hermes_skill_dir::HermesSkillDir;
use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::process_executor::ProcessExecutor;

/// The plugin's skill directories Hermes should be able to discover.
pub const HERMES_SKILL_NAMES: [&str; 13] = [
    "kmp-catchup",
    "kmp-doctor",
    "kmp-expert",
    "kmp-guide",
    "kmp-info",
    "kmp-lifecycle",
    "kmp-memory",
    "kmp-moves",
    "kmp-restore",
    "kmp-revert",
    "kmp-save",
    "kmp-setup",
    "kmp-uninstall",
];

/// Adapter for the Hermes Agent host.
///
/// Hermes has no plugin manager: its MCP registrations live in its own
/// config, reached only through `hermes config` / `hermes mcp add`, and its
/// skills are plain directories under `$HERMES_HOME/skills`. This adapter
/// reads that evidence and performs the two mutations a convergence needs —
/// registering the engine by its PATH name, the way Codex consumes it, and
/// mirroring the plugin's skills where Hermes scans.
pub struct HermesHostAdapter<'a> {
    processes: &'a dyn ProcessExecutor,
    skills: HermesSkillDir,
}

impl<'a> HermesHostAdapter<'a> {
    pub fn new(processes: &'a dyn ProcessExecutor) -> Result<Self, LifecycleError> {
        let home = std::env::var_os("HERMES_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".hermes")))
            .ok_or_else(|| {
                LifecycleError::HostNotInstalled(
                    "no HOME resolves, so no Hermes skills directory can be named".to_string(),
                )
            })?;
        Ok(Self {
            processes,
            skills: HermesSkillDir::from_home_dir(home)?,
        })
    }

    pub fn with_skill_dir(processes: &'a dyn ProcessExecutor, skills: HermesSkillDir) -> Self {
        Self { processes, skills }
    }

    /// The command Hermes would run for `kmp`, or none when unregistered.
    pub fn registered_command(&self) -> Result<Option<PathBuf>, LifecycleError> {
        let config = self.mcp_servers_config()?;
        match HermesRuntimeStatusMapper::map(&config)? {
            crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus::Registered => {
                Ok(Self::command_from(&config))
            }
            crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus::Failed(detail) => {
                Err(LifecycleError::InvalidHostResponse(detail))
            }
            _ => Ok(None),
        }
    }

    fn command_from(config_yaml: &str) -> Option<PathBuf> {
        let body: serde_yaml::Value = serde_yaml::from_str(config_yaml).ok()?;
        let command = body.get("kmp")?.get("command")?.as_str()?;
        Some(PathBuf::from(command))
    }

    fn mcp_servers_config(&self) -> Result<String, LifecycleError> {
        let output = self
            .processes
            .execute(Host::Hermes.executable(), &["config", "get", "mcp_servers"])?
            .require_success(Host::Hermes.executable())
            .map_err(|detail| LifecycleError::CommandFailed {
                program: Host::Hermes.executable().to_string(),
                detail,
            })?;
        Ok(output.stdout().to_string())
    }

    /// Which plugin skills Hermes can already discover.
    pub fn installed_skills(&self) -> Vec<String> {
        let names: Vec<String> = HERMES_SKILL_NAMES.iter().map(ToString::to_string).collect();
        self.skills.installed_skills(&names)
    }

    /// The installation Hermes declares: the MCP registration plus the
    /// discoverable skills, rooted at the Hermes home that owns both. None
    /// when Hermes holds no KMP surface at all.
    pub fn installation(
        &self,
        version: &ReleaseVersion,
    ) -> Result<Option<HostInstallation>, LifecycleError> {
        let registration = self.registered_command()?;
        let skills = self.installed_skills();
        if registration.is_none() && skills.is_empty() {
            return Ok(None);
        }
        let enabled = registration.as_ref().is_some_and(|command| {
            command.ends_with("kmp-mcp") || Self::is_plugin_launcher(command)
        });
        Ok(Some(HostInstallation::discovered(
            Host::Hermes,
            version.clone(),
            self.hermes_home()?,
            enabled,
        )))
    }

    fn is_plugin_launcher(command: &Path) -> bool {
        command
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("run-embedded-mcp"))
    }

    fn hermes_home(&self) -> Result<PluginRoot, LifecycleError> {
        let skills = self.skills.as_path();
        let home = skills
            .parent()
            .ok_or_else(|| LifecycleError::UnsafePath(skills.to_path_buf()))?;
        PluginRoot::new(home)
    }

    /// Register the KMP engine through Hermes' own CLI, then mirror the
    /// plugin's skills from `plugin_root` when one is available. Registration
    /// is non-interactive by pinning `echo Y` to the tool-enable prompt — the
    /// documented pitfall — and the CLI's own confirmation is required.
    pub fn provision(
        &self,
        version: &ReleaseVersion,
        plugin_root: Option<&Path>,
    ) -> Result<HostInstallation, LifecycleError> {
        if self.registered_command()?.is_none() {
            let output = self
                .processes
                .execute(
                    "sh",
                    &["-c", "echo Y | hermes mcp add kmp --command kmp-mcp"],
                )?
                .require_success(Host::Hermes.executable())
                .map_err(|detail| LifecycleError::CommandFailed {
                    program: Host::Hermes.executable().to_string(),
                    detail,
                })?;
            if !output.stdout().contains("Saved 'kmp'") {
                return Err(LifecycleError::InvalidHostResponse(format!(
                    "Hermes did not confirm the kmp MCP registration: {}",
                    output.diagnostic()
                )));
            }
        }
        if let Some(plugin_root) = plugin_root {
            self.mirror_skills(plugin_root)?;
        }
        self.installation(version)?.ok_or_else(|| {
            LifecycleError::InvalidHostResponse(
                "Hermes reported neither a registration nor skills after provisioning".to_string(),
            )
        })
    }

    /// Keep the registration; converge the skill mirror to the plugin tree.
    pub fn refresh(
        &self,
        version: &ReleaseVersion,
        plugin_root: Option<&Path>,
    ) -> Result<HostInstallation, LifecycleError> {
        if let Some(plugin_root) = plugin_root {
            self.mirror_skills(plugin_root)?;
        }
        self.installation(version)?.ok_or_else(|| {
            LifecycleError::InvalidHostResponse(
                "Hermes reported neither a registration nor skills after a refresh".to_string(),
            )
        })
    }

    /// Skills are plain directories with a `SKILL.md` under
    /// `$HERMES_HOME/skills`; mirroring is the whole skill surface.
    #[cfg(test)]
    fn mirror_skills_for_test(&self, plugin_root: &Path) -> Result<(), LifecycleError> {
        self.mirror_skills(plugin_root)
    }

    fn mirror_skills(&self, plugin_root: &Path) -> Result<(), LifecycleError> {
        fs::create_dir_all(self.skills.as_path()).map_err(|error| {
            LifecycleError::HostNotInstalled(format!(
                "Hermes skills directory could not be created: {error}"
            ))
        })?;
        for name in HERMES_SKILL_NAMES {
            let source = plugin_root.join("skills").join(name);
            if !source.join("SKILL.md").is_file() {
                continue;
            }
            let destination = self.skills.skill(name);
            if destination.exists() {
                fs::remove_dir_all(&destination).map_err(|error| {
                    LifecycleError::HostNotInstalled(format!(
                        "Hermes skill {name} could not be replaced: {error}"
                    ))
                })?;
            }
            Self::copy_tree(&source, &destination)?;
        }
        Ok(())
    }

    fn copy_tree(source: &Path, destination: &Path) -> Result<(), LifecycleError> {
        fs::create_dir_all(destination).map_err(|error| {
            LifecycleError::HostNotInstalled(format!(
                "Hermes skill directory could not be created: {error}"
            ))
        })?;
        for entry in fs::read_dir(source).map_err(|error| {
            LifecycleError::HostNotInstalled(format!("Hermes skill source unreadable: {error}"))
        })? {
            let entry = entry.map_err(|error| {
                LifecycleError::HostNotInstalled(format!("Hermes skill entry unreadable: {error}"))
            })?;
            let target = destination.join(entry.file_name());
            let is_dir = entry
                .file_type()
                .map_err(|error| {
                    LifecycleError::HostNotInstalled(format!(
                        "Hermes skill entry type unreadable: {error}"
                    ))
                })?
                .is_dir();
            if is_dir {
                Self::copy_tree(&entry.path(), &target)?;
            } else {
                fs::copy(entry.path(), &target).map_err(|error| {
                    LifecycleError::HostNotInstalled(format!(
                        "Hermes skill file could not be copied: {error}"
                    ))
                })?;
            }
        }
        Ok(())
    }

    /// Hermes consumes the shared engine by its PATH name, exactly as Codex
    /// does, so the runtime engine is whatever PATH resolves.
    pub fn runtime_engine(&self) -> Result<EngineExecutable, LifecycleError> {
        self.processes
            .resolve("kmp-mcp")
            .map(EngineExecutable::installed_at)
            .ok_or_else(|| {
                LifecycleError::HostNotInstalled(
                    "Hermes declares `kmp-mcp`, but no executable resolves on PATH".to_string(),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::lifecycle::adapters::tests_support::FakeProcessExecutor;
    use crate::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
    use crate::lifecycle::ports::process_output::ProcessOutput;

    fn command(
        program: &str,
        arguments: &[&str],
        success: bool,
        stdout: &str,
    ) -> (String, Vec<String>, ProcessOutput) {
        (
            program.to_string(),
            arguments.iter().map(ToString::to_string).collect(),
            ProcessOutput::completed(success, stdout.to_string(), String::new()),
        )
    }

    fn version() -> ReleaseVersion {
        ReleaseVersion::parse("0.18.9").expect("version")
    }

    fn skill_dir_with(names: &[&str]) -> (tempfile::TempDir, HermesSkillDir) {
        let root = tempfile::tempdir().expect("tempdir");
        let skills = HermesSkillDir::new(root.path().join("hermes/skills")).expect("skills");
        for name in names {
            let dir = skills.skill(name);
            fs::create_dir_all(&dir).expect("skill dir");
            fs::write(dir.join("SKILL.md"), "---\nname: x\n---").expect("skill md");
        }
        (root, skills)
    }

    #[test]
    fn an_unconfigured_hermes_declares_no_installation() {
        let config = command(
            "hermes",
            &["config", "get", "mcp_servers"],
            true,
            "other:\n  command: uvx\n  enabled: true\n",
        );
        // installation() reads the config surface twice: once for the
        // registration, once through the same gate. The fake serves each
        // read once.
        let processes = FakeProcessExecutor::expecting(vec![config.clone(), config]);
        let (_root, skills) = skill_dir_with(&[]);
        let adapter = HermesHostAdapter::with_skill_dir(&processes, skills);

        assert_eq!(adapter.registered_command().expect("command"), None);
        assert!(
            adapter
                .installation(&version())
                .expect("installation")
                .is_none()
        );
        assert!(processes.is_exhausted());
    }

    #[test]
    fn a_registration_through_the_path_engine_is_an_enabled_installation() {
        let processes = FakeProcessExecutor::expecting(vec![command(
            "hermes",
            &["config", "get", "mcp_servers"],
            true,
            "kmp:\n  command: kmp-mcp\n  enabled: true\n",
        )]);
        let (_root, skills) = skill_dir_with(&["kmp-doctor", "kmp-guide"]);
        let adapter = HermesHostAdapter::with_skill_dir(&processes, skills);

        let installation = adapter
            .installation(&version())
            .expect("installation")
            .expect("Hermes holds a KMP surface");
        assert_eq!(installation.host(), Host::Hermes);
        assert!(installation.is_enabled());
        assert_eq!(
            adapter.installed_skills(),
            vec!["kmp-doctor".to_string(), "kmp-guide".to_string()]
        );
        assert!(processes.is_exhausted());
    }

    #[test]
    fn a_foreign_command_registers_but_disables_the_convergence() {
        let processes = FakeProcessExecutor::expecting(vec![command(
            "hermes",
            &["config", "get", "mcp_servers"],
            true,
            "kmp:\n  command: /usr/bin/some-other-kmp\n  enabled: true\n",
        )]);
        let (_root, skills) = skill_dir_with(&[]);
        let adapter = HermesHostAdapter::with_skill_dir(&processes, skills);

        let installation = adapter
            .installation(&version())
            .expect("installation")
            .expect("a registration exists");
        assert!(!installation.is_enabled());
        assert!(processes.is_exhausted());
    }

    #[test]
    fn a_plugin_launcher_registration_is_enabled_too() {
        let processes = FakeProcessExecutor::expecting(vec![command(
            "hermes",
            &["config", "get", "mcp_servers"],
            true,
            "kmp:\n  command: /opt/kmp/run-embedded-mcp.sh\n  enabled: true\n",
        )]);
        let (_root, skills) = skill_dir_with(&[]);
        let adapter = HermesHostAdapter::with_skill_dir(&processes, skills);

        let installation = adapter
            .installation(&version())
            .expect("installation")
            .expect("a registration exists");
        assert!(installation.is_enabled());
    }

    #[test]
    fn mirroring_brings_the_plugin_skills_where_hermes_scans() {
        let plugin = tempfile::tempdir().expect("plugin root");
        for name in ["kmp-doctor", "kmp-guide", "kmp-memory"] {
            let dir = plugin.path().join("skills").join(name);
            fs::create_dir_all(&dir).expect("plugin skill dir");
            fs::write(dir.join("SKILL.md"), "---\nname: x\n---").expect("plugin skill md");
        }
        let home = tempfile::tempdir().expect("hermes home");
        let skills = HermesSkillDir::new(home.path().join("skills")).expect("skills");
        let adapter = HermesHostAdapter {
            processes: &FakeProcessExecutor::expecting(vec![]),
            skills,
        };

        adapter
            .mirror_skills_for_test(plugin.path())
            .expect("mirror");

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
    fn the_runtime_engine_is_what_path_resolves() {
        let processes = FakeProcessExecutor::expecting(vec![]);
        let (_root, skills) = skill_dir_with(&[]);
        let adapter = HermesHostAdapter::with_skill_dir(&processes, skills);
        // PATH in the test process resolves through the fake, so the engine is
        // named; a real machine without the binary gets the same failure shape.
        let engine = adapter.runtime_engine().expect("engine");
        assert!(engine.as_path().ends_with("kmp-mcp"));
    }

    #[test]
    fn hermes_runtime_status_reads_the_config_surface() {
        let status = HermesRuntimeStatusMapper::map("kmp:\n  command: kmp-mcp\n  enabled: true\n")
            .expect("status");
        assert_eq!(status, HostRuntimeStatus::Registered);
    }
}
