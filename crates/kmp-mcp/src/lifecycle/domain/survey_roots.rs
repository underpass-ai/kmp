use std::path::PathBuf;

/// Where to look. Taken as values rather than read from the environment so
/// the survey can be exercised against a temporary tree.
pub struct SurveyRoots {
    pub home: PathBuf,
    pub data_home: PathBuf,
    pub working_dir: PathBuf,
    pub path_entries: Vec<PathBuf>,
}

impl SurveyRoots {
    /// Every directory a `kmp-mcp` could be found in, in the order a shell
    /// would meet them.
    ///
    /// `PATH` comes first, in its own order, because that order is the whole
    /// question: it decides which engine a bare `kmp-mcp` runs, and rustup's
    /// env ordering putting `~/.cargo/bin` first is how a twenty-releases-old
    /// engine ends up answering against a current store (#450). The
    /// conventional install directories follow, so an engine `PATH` does not
    /// carry is still seen.
    pub fn engine_directories(&self) -> Vec<PathBuf> {
        let mut directories = self.path_entries.clone();
        directories.extend([
            self.home.join(".local/bin"),
            self.home.join(".cargo/bin"),
            self.data_home.join("kmp/bin"),
        ]);
        let mut seen = Vec::new();
        directories.retain(|directory| {
            let first = !seen.contains(directory);
            if first {
                seen.push(directory.clone());
            }
            first
        });
        directories
    }

    /// Whether `PATH` itself carries this directory. A conventional install
    /// directory outside `PATH` holds an engine nothing resolves to.
    pub fn is_on_path(&self, directory: &std::path::Path) -> bool {
        self.path_entries.iter().any(|entry| entry == directory)
    }
}

/// The engine's file name on this platform.
pub fn engine_file_name() -> &'static str {
    if cfg!(windows) {
        "kmp-mcp.exe"
    } else {
        "kmp-mcp"
    }
}
