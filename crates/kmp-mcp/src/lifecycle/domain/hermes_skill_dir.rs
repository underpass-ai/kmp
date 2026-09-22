use std::path::{Path, PathBuf};

use super::lifecycle_error::LifecycleError;

/// The user skills directory Hermes Agent actually reads at startup.
///
/// Hermes discovers skills by scanning `$HERMES_HOME/skills` for directories
/// carrying a `SKILL.md`; there is no plugin registry to consult and no CLI
/// that lists them. Naming the directory here — rather than passing paths
/// around — keeps that contract in one place, and `$HERMES_HOME` resolves the
/// way Hermes itself resolves it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HermesSkillDir(PathBuf);

impl HermesSkillDir {
    /// From the machine's Hermes home, defaulting to `~/.hermes`.
    pub fn under_home(home: impl AsRef<Path>) -> Result<Self, LifecycleError> {
        Self::from_home_dir(home.as_ref().join(".hermes"))
    }

    pub fn from_home_dir(home: impl AsRef<Path>) -> Result<Self, LifecycleError> {
        Self::new(home.as_ref().join("skills"))
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

    /// The directory one installed skill would occupy.
    pub fn skill(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// Which of the plugin's skills are already discoverable by Hermes.
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
    fn hermes_skills_live_directly_under_the_hermes_home() {
        let dir = HermesSkillDir::from_home_dir("/home/reader/.hermes").expect("skill dir");
        assert_eq!(dir.as_path(), Path::new("/home/reader/.hermes/skills"));
        assert_eq!(
            dir.skill("kmp-doctor"),
            PathBuf::from("/home/reader/.hermes/skills/kmp-doctor")
        );
    }

    #[test]
    fn a_skill_is_installed_only_when_its_skill_md_is_present() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = HermesSkillDir::new(root.path().join(".hermes/skills")).expect("skill dir");
        std::fs::create_dir_all(dir.skill("kmp-doctor")).expect("skill directory");
        std::fs::write(
            dir.skill("kmp-doctor").join("SKILL.md"),
            "---\nname: kmp-doctor\n---",
        )
        .expect("skill file");
        std::fs::create_dir_all(dir.skill("kmp-guide")).expect("empty skill directory");

        assert_eq!(
            dir.installed_skills(&names(&["kmp-doctor", "kmp-guide", "kmp-memory"])),
            names(&["kmp-doctor"])
        );
    }

    #[test]
    fn a_relative_directory_is_not_a_skill_dir_this_process_may_touch() {
        assert!(HermesSkillDir::new("./skills").is_err());
    }
}
