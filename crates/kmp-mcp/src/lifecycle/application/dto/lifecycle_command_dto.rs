use std::path::PathBuf;

/// CLI-facing lifecycle input before domain validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleCommandDto {
    pub hosts: Vec<String>,
    pub version: Option<String>,
    pub install_dir: PathBuf,
    pub dry_run: bool,
    /// A lexical-bridge table the operator built, to install instead of the
    /// one the release publishes.
    pub lexical_bridge: Option<PathBuf>,
    /// Whether the operator asked for no table at all.
    pub decline_bridge: bool,
    /// Where the machine's table goes, when this platform has a data home to
    /// put it in.
    pub bridge_dir: Option<PathBuf>,
}
