use std::path::PathBuf;

/// CLI-facing lifecycle input before domain validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleCommandDto {
    pub hosts: Vec<String>,
    pub version: Option<String>,
    pub install_dir: PathBuf,
    pub dry_run: bool,
}
