//! What a piece of an installation is.
//!
//! One concept: the kinds, and the word each one is called by in the report.
//! The kind is what decides whether a piece may be removed, saved first, or
//! only named — so it is a type, not a string.

/// What a piece of an installation is, which decides how it is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    /// A `kmp-mcp` executable.
    Engine,
    /// A data directory holding memory.
    Store,
    /// A committed event-log bundle. Removing memory does not remove this:
    /// it lives in the repository and belongs to whoever committed it.
    Bundle,
    /// Plugin files a host reads.
    HostFiles,
    /// A registration inside a host's own configuration file.
    HostWiring,
}

impl PieceKind {
    pub(in crate::uninstall) fn label(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::Store => "memory",
            Self::Bundle => "bundle",
            Self::HostFiles => "host files",
            Self::HostWiring => "host wiring",
        }
    }
}
