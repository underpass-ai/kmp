use std::fs;
use std::path::Path;

use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;

/// The plugin's skill directories a host without a plugin manager should be
/// able to discover. Hermes and Pi both scan plain skill directories.
pub const NATIVE_SKILL_NAMES: [&str; 13] = [
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

/// Copies the plugin's skills into the directory a native host scans.
///
/// A skill is a plain directory with a `SKILL.md`; mirroring is the whole
/// skill surface for hosts that have no plugin manager. Each mirrored skill
/// replaces the previous copy so a convergence never leaves stale files.
#[derive(Clone, Copy, Debug)]
pub struct NativeSkillMirror {
    host: Host,
}

impl NativeSkillMirror {
    pub fn for_host(host: Host) -> Self {
        Self { host }
    }

    /// The names every native host mirrors, as owned strings.
    pub fn skill_names() -> Vec<String> {
        NATIVE_SKILL_NAMES.iter().map(ToString::to_string).collect()
    }

    pub fn mirror(&self, plugin_root: &Path, skills_dir: &Path) -> Result<(), LifecycleError> {
        fs::create_dir_all(skills_dir).map_err(|error| {
            self.failure(format!("skills directory could not be created: {error}"))
        })?;
        for name in NATIVE_SKILL_NAMES {
            let source = plugin_root.join("skills").join(name);
            if !source.join("SKILL.md").is_file() {
                continue;
            }
            let destination = skills_dir.join(name);
            if destination.exists() {
                fs::remove_dir_all(&destination).map_err(|error| {
                    self.failure(format!("skill {name} could not be replaced: {error}"))
                })?;
            }
            self.copy_tree(&source, &destination)?;
        }
        Ok(())
    }

    fn copy_tree(&self, source: &Path, destination: &Path) -> Result<(), LifecycleError> {
        fs::create_dir_all(destination).map_err(|error| {
            self.failure(format!("skill directory could not be created: {error}"))
        })?;
        for entry in fs::read_dir(source)
            .map_err(|error| self.failure(format!("skill source unreadable: {error}")))?
        {
            let entry =
                entry.map_err(|error| self.failure(format!("skill entry unreadable: {error}")))?;
            let target = destination.join(entry.file_name());
            let is_dir = entry
                .file_type()
                .map_err(|error| self.failure(format!("skill entry type unreadable: {error}")))?
                .is_dir();
            if is_dir {
                self.copy_tree(&entry.path(), &target)?;
            } else {
                fs::copy(entry.path(), &target).map_err(|error| {
                    self.failure(format!("skill file could not be copied: {error}"))
                })?;
            }
        }
        Ok(())
    }

    fn failure(&self, detail: String) -> LifecycleError {
        LifecycleError::HostNotInstalled(format!("{} {detail}", self.host))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_with(names: &[&str]) -> tempfile::TempDir {
        let plugin = tempfile::tempdir().expect("plugin root");
        for name in names {
            let dir = plugin.path().join("skills").join(name).join("references");
            fs::create_dir_all(&dir).expect("plugin skill dir");
            fs::write(dir.parent().expect("skill").join("SKILL.md"), "---\n---").expect("md");
            fs::write(dir.join("verbs.md"), "verbs").expect("reference");
        }
        plugin
    }

    #[test]
    fn mirroring_copies_whole_skill_trees_and_skips_what_the_plugin_lacks() {
        let plugin = plugin_with(&["kmp-doctor", "not-a-kmp-skill"]);
        let home = tempfile::tempdir().expect("home");
        let skills = home.path().join("skills");

        NativeSkillMirror::for_host(Host::Pi)
            .mirror(plugin.path(), &skills)
            .expect("mirror");

        assert!(skills.join("kmp-doctor/references/verbs.md").is_file());
        assert!(!skills.join("kmp-guide").exists());
        assert!(!skills.join("not-a-kmp-skill").exists());
    }

    #[test]
    fn mirroring_replaces_a_stale_copy() {
        let plugin = plugin_with(&["kmp-doctor"]);
        let home = tempfile::tempdir().expect("home");
        let skills = home.path().join("skills");
        fs::create_dir_all(skills.join("kmp-doctor")).expect("stale skill");
        fs::write(skills.join("kmp-doctor/stale.md"), "old").expect("stale file");

        NativeSkillMirror::for_host(Host::Hermes)
            .mirror(plugin.path(), &skills)
            .expect("mirror");

        assert!(!skills.join("kmp-doctor/stale.md").exists());
        assert!(skills.join("kmp-doctor/SKILL.md").is_file());
    }

    #[test]
    fn a_failure_names_the_host_it_happened_to() {
        let plugin = plugin_with(&["kmp-doctor"]);
        let home = tempfile::tempdir().expect("home");
        let blocker = home.path().join("skills");
        fs::write(&blocker, "a file where the directory should be").expect("blocker");

        let error = NativeSkillMirror::for_host(Host::Pi)
            .mirror(plugin.path(), &blocker)
            .expect_err("a file cannot hold skills");

        assert!(error.to_string().contains("pi"), "{error}");
    }

    #[test]
    fn the_shared_names_are_the_plugin_skills() {
        assert_eq!(
            NativeSkillMirror::skill_names().len(),
            NATIVE_SKILL_NAMES.len()
        );
        assert!(NativeSkillMirror::skill_names().contains(&"kmp-memory".to_string()));
    }
}
