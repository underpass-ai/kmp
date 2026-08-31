use crate::lifecycle::domain::engine_executable::EngineExecutable;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// One `kmp-mcp` this machine actually carries, and how it was reached.
///
/// A second copy is how a live session ends up older than the merged fix
/// (#80). Naming the copy is not enough to see that hazard: what matters is
/// its release and whether a bare `kmp-mcp` resolves to it, because those two
/// facts together are the difference between a harmless leftover and an
/// ancient engine answering against a store it does not understand (#450).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoundEngine {
    executable: EngineExecutable,
    version: Option<ReleaseVersion>,
    selected_by_path: bool,
}

impl FoundEngine {
    pub fn new(
        executable: EngineExecutable,
        version: Option<ReleaseVersion>,
        selected_by_path: bool,
    ) -> Self {
        Self {
            executable,
            version,
            selected_by_path,
        }
    }

    pub fn executable(&self) -> &EngineExecutable {
        &self.executable
    }

    /// The release it answers `--version` with, or nothing when it would not
    /// say — a file named `kmp-mcp` that is not one, or one too broken to ask.
    pub fn version(&self) -> Option<&ReleaseVersion> {
        self.version.as_ref()
    }

    /// Whether `PATH` order makes this the engine a bare `kmp-mcp` runs.
    pub fn selected_by_path(&self) -> bool {
        self.selected_by_path
    }

    /// A release the reader can read, or the honest absence of one.
    pub fn described_version(&self) -> String {
        self.version
            .as_ref()
            .map_or_else(|| "unknown version".to_string(), |v| v.as_str().to_string())
    }

    pub fn matches(&self, target: &ReleaseVersion) -> bool {
        self.version
            .as_ref()
            .is_some_and(|version| version.represents_same_release(target))
    }
}
