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
    /// Files a retired way of installing KMP left behind. Nothing running
    /// reads them, and telling them apart from live wiring is the whole
    /// point: a reader deciding what to keep cannot do it from a list where
    /// the plugin tree that serves them and the prompts of a wiring that was
    /// retired look alike.
    Leftover,
    /// A registration inside a host's own configuration file.
    HostWiring,
}

impl PieceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::Store => "memory",
            Self::Bundle => "bundle",
            Self::HostFiles => "host files",
            Self::Leftover => "leftover",
            Self::HostWiring => "host wiring",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PieceKind;

    #[test]
    fn a_leftover_is_labelled_apart_from_the_host_files_that_are_in_use() {
        assert_eq!(PieceKind::Leftover.label(), "leftover");
        assert_ne!(PieceKind::Leftover.label(), PieceKind::HostFiles.label());
    }
}
