use std::path::{Path, PathBuf};

use super::lifecycle_error::LifecycleError;

/// The Pi coding agent's home: the directory holding its `settings.json` and
/// the `skills/` it scans at startup.
///
/// Pi resolves it from `$PI_CODING_AGENT_DIR`, defaulting to `~/.pi/agent`.
/// Pi has no native MCP: the KMP connection is the `underpass-pi` package
/// listed in `settings.json`, so both the registration evidence and the skill
/// surface are named from this one place.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PiAgentHome(PathBuf);

impl PiAgentHome {
    /// From the user's home directory, the way Pi defaults it.
    pub fn under_home(home: impl AsRef<Path>) -> Result<Self, LifecycleError> {
        Self::new(home.as_ref().join(".pi").join("agent"))
    }

    pub fn new(path: impl Into<PathBuf>) -> Result<Self, LifecycleError> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(LifecycleError::UnsafePath(path));
        }
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Where Pi records its installed packages.
    pub fn settings_file(&self) -> PathBuf {
        self.0.join("settings.json")
    }

    /// The user skills directory Pi scans.
    pub fn skills_dir(&self) -> PathBuf {
        self.0.join("skills")
    }

    /// The directory one installed skill would occupy.
    pub fn skill(&self, name: &str) -> PathBuf {
        self.skills_dir().join(name)
    }

    /// Which of the plugin's skills are already discoverable by Pi.
    pub fn installed_skills(&self, names: &[String]) -> Vec<String> {
        names
            .iter()
            .filter(|name| self.skill(name).join("SKILL.md").is_file())
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(entries: &[&str]) -> Vec<String> {
        entries.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn the_default_home_is_the_agent_directory_under_dot_pi() {
        let home = PiAgentHome::under_home("/home/reader").expect("home");
        assert_eq!(home.as_path(), Path::new("/home/reader/.pi/agent"));
        assert_eq!(
            home.settings_file(),
            PathBuf::from("/home/reader/.pi/agent/settings.json")
        );
        assert_eq!(
            home.skill("kmp-doctor"),
            PathBuf::from("/home/reader/.pi/agent/skills/kmp-doctor")
        );
    }

    #[test]
    fn a_skill_is_installed_only_when_its_skill_md_is_present() {
        let root = tempfile::tempdir().expect("tempdir");
        let home = PiAgentHome::new(root.path().join("agent")).expect("home");
        std::fs::create_dir_all(home.skill("kmp-doctor")).expect("skill directory");
        std::fs::write(home.skill("kmp-doctor").join("SKILL.md"), "---\n---").expect("md");
        std::fs::create_dir_all(home.skill("kmp-guide")).expect("empty skill directory");

        assert_eq!(
            home.installed_skills(&names(&["kmp-doctor", "kmp-guide", "kmp-memory"])),
            names(&["kmp-doctor"])
        );
    }

    #[test]
    fn a_relative_directory_is_not_a_home_this_process_may_touch() {
        assert!(PiAgentHome::new("./agent").is_err());
    }
}
