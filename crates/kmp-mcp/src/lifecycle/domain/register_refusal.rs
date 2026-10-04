use std::fmt;
use std::path::PathBuf;

/// Why `kmp-mcp memories register` would not add a path to the index.
///
/// Registering only remembers a store that already exists: nothing is
/// created, and a path that is not exactly one store is refused rather than
/// guessed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterRefusal {
    /// `~` is the shell's to expand; a stored `~` would mean nothing later.
    UnexpandedHome(String),
    /// A relative path depends on where the command ran.
    Relative(String),
    /// No `FORMAT_VERSION` stamp at the path: not a store.
    NotAStore(PathBuf),
}

impl fmt::Display for RegisterRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpandedHome(raw) => write!(
                f,
                "`{raw}` starts with `~`, which only a shell expands; pass the absolute path"
            ),
            Self::Relative(raw) => write!(
                f,
                "`{raw}` is relative; pass the absolute path of the store directory"
            ),
            Self::NotAStore(path) => write!(
                f,
                "`{}` is not a KMP store: there is no `{}`. Nothing was created or remembered",
                path.display(),
                path.join("FORMAT_VERSION").display()
            ),
        }
    }
}
