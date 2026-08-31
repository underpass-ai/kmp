use std::path::PathBuf;

use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::plugin_cache::PluginCache;

/// A host plugin cache on disk: `<cache>/<marketplace>/<plugin>/<version>`.
///
/// The installed root *is* a version directory, so its parent holds the
/// siblings. Nothing here walks outward from that parent, and a directory
/// whose name is not a release is left alone: this cache is a host's, not
/// KMP's, and an unrecognized entry is somebody else's business.
#[derive(Clone, Copy, Debug, Default)]
pub struct FilesystemPluginCache;

impl FilesystemPluginCache {
    fn versions_dir(installed: &PluginRoot) -> Result<PathBuf, LifecycleError> {
        installed
            .as_path()
            .parent()
            .map(PathBuf::from)
            .ok_or_else(|| LifecycleError::UnsafePath(installed.as_path().to_path_buf()))
    }
}

impl PluginCache for FilesystemPluginCache {
    fn cached_releases(
        &self,
        installed: &PluginRoot,
    ) -> Result<Vec<ReleaseVersion>, LifecycleError> {
        let versions = Self::versions_dir(installed)?;
        let entries = match std::fs::read_dir(&versions) {
            Ok(entries) => entries,
            // A cache that is not there holds nothing; that is an answer, not
            // a failure, and an update must not fail over housekeeping.
            Err(_) => return Ok(Vec::new()),
        };
        Ok(entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| ReleaseVersion::parse(entry.file_name().to_str()?).ok())
            .collect())
    }

    fn remove_release(
        &self,
        installed: &PluginRoot,
        release: &ReleaseVersion,
    ) -> Result<(), LifecycleError> {
        let versions = Self::versions_dir(installed)?;
        let target = versions.join(release.as_str());
        // Exactly the sibling this release names, never the installed one and
        // never a path a `..` could have built.
        if target == installed.as_path() || target.parent() != Some(versions.as_path()) {
            return Err(LifecycleError::UnsafePath(target));
        }
        std::fs::remove_dir_all(&target).map_err(|error| LifecycleError::Io {
            path: target,
            detail: error.to_string(),
        })
    }
}
