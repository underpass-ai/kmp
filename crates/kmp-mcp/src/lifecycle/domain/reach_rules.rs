use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::store_reach::StoreReach;

/// The rules that decide how each store on the machine is reached, taken as
/// values so the decision can be exercised without an environment.
///
/// The store this process would open is labelled with the rule that actually
/// chose it. Every other store is labelled by the rule that *would* reach it
/// from somewhere: the saved selection from any directory without a project,
/// the per-user default from anywhere with nothing set, a `.kernel` from its
/// own directory. Anything else is reached by no rule at all.
#[derive(Debug, Clone)]
pub struct ReachRules {
    opened_here: Option<(PathBuf, StoreReach)>,
    saved: Option<PathBuf>,
    user_default: PathBuf,
}

/// The directory a project store is kept in, relative to its project root.
const PROJECT_STORE_DIR: &str = ".kernel";

impl ReachRules {
    /// Rules that know only where the per-user default is.
    pub fn new(user_default: PathBuf) -> Self {
        Self {
            opened_here: None,
            saved: None,
            user_default,
        }
    }

    /// The store this process would open, and the rule that chose it.
    pub fn opening(mut self, path: PathBuf, reach: StoreReach) -> Self {
        self.opened_here = Some((path, reach));
        self
    }

    /// The selection saved in the user config file.
    pub fn saved(mut self, path: PathBuf) -> Self {
        self.saved = Some(path);
        self
    }

    /// Whether this process would open exactly this store.
    pub fn is_opened_here(&self, path: &Path) -> bool {
        self.opened_here
            .as_ref()
            .is_some_and(|(opened, _)| opened.as_path() == path)
    }

    /// How this store is reached.
    pub fn reach_of(&self, path: &Path) -> StoreReach {
        if let Some((opened, reach)) = &self.opened_here
            && opened.as_path() == path
        {
            return *reach;
        }
        if self.saved.as_deref() == Some(path) {
            return StoreReach::Saved;
        }
        if self.user_default.as_path() == path {
            return StoreReach::User;
        }
        if path.file_name() == Some(OsStr::new(PROJECT_STORE_DIR)) {
            return StoreReach::Project;
        }
        StoreReach::Unreachable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> ReachRules {
        ReachRules::new(PathBuf::from("/data/kmp/default"))
    }

    #[test]
    fn the_store_opened_here_carries_the_rule_that_chose_it() {
        for reach in [
            StoreReach::Env,
            StoreReach::Project,
            StoreReach::Worktree,
            StoreReach::Saved,
            StoreReach::User,
            StoreReach::UserFallback,
        ] {
            let rules = rules().opening(PathBuf::from("/anywhere/store"), reach);
            assert_eq!(rules.reach_of(Path::new("/anywhere/store")), reach);
            assert!(rules.is_opened_here(Path::new("/anywhere/store")));
        }
    }

    #[test]
    fn an_env_store_under_the_data_home_is_env_rather_than_unreachable() {
        let rules = rules().opening(PathBuf::from("/data/kmp/shared"), StoreReach::Env);
        assert_eq!(
            rules.reach_of(Path::new("/data/kmp/shared")),
            StoreReach::Env
        );
    }

    #[test]
    fn the_saved_selection_is_saved_even_when_another_store_opens_here() {
        let rules = rules()
            .opening(PathBuf::from("/repo/.kernel"), StoreReach::Project)
            .saved(PathBuf::from("/games/ships/.kernel"));
        assert_eq!(
            rules.reach_of(Path::new("/games/ships/.kernel")),
            StoreReach::Saved,
            "a remembered .kernel that is the saved selection is reached by the selection"
        );
        assert!(!rules.is_opened_here(Path::new("/games/ships/.kernel")));
    }

    #[test]
    fn the_per_user_default_is_user_when_something_else_opens_here() {
        let rules = rules().opening(PathBuf::from("/repo/.kernel"), StoreReach::Project);
        assert_eq!(
            rules.reach_of(Path::new("/data/kmp/default")),
            StoreReach::User
        );
    }

    #[test]
    fn a_remembered_kernel_directory_is_reached_from_its_project() {
        assert_eq!(
            rules().reach_of(Path::new("/elsewhere/repo/.kernel")),
            StoreReach::Project
        );
    }

    #[test]
    fn anything_else_is_reached_by_no_rule() {
        assert_eq!(
            rules().reach_of(Path::new("/data/kmp/retired-2026-08-17")),
            StoreReach::Unreachable
        );
        assert_eq!(
            rules().reach_of(Path::new("/scratch/tmp/store")),
            StoreReach::Unreachable
        );
    }
}
