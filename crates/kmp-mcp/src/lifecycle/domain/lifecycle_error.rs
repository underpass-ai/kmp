use std::fmt;
use std::path::PathBuf;

/// Typed lifecycle failure. No message parsing decides control flow.
#[derive(Debug)]
pub enum LifecycleError {
    CommandFailed { program: String, detail: String },
    HostNotInstalled(String),
    HostVersionMismatch(String),
    InvalidCommand(String),
    InvalidHostResponse(String),
    InvalidReleaseVersion(String),
    Io { path: PathBuf, detail: String },
    Network(String),
    NoInstalledHost,
    SurfaceMismatch(String),
    TreeMismatch(String),
    UnsafePath(PathBuf),
    UnsupportedPlatform(String),
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandFailed { program, detail } => {
                write!(formatter, "{program} failed: {detail}")
            }
            Self::HostNotInstalled(detail)
            | Self::HostVersionMismatch(detail)
            | Self::InvalidCommand(detail)
            | Self::InvalidHostResponse(detail)
            | Self::Network(detail)
            | Self::SurfaceMismatch(detail)
            | Self::TreeMismatch(detail)
            | Self::UnsupportedPlatform(detail) => formatter.write_str(detail),
            Self::InvalidReleaseVersion(version) => {
                write!(formatter, "invalid KMP release version `{version}`")
            }
            Self::Io { path, detail } => write!(formatter, "{}: {detail}", path.display()),
            Self::NoInstalledHost => {
                formatter.write_str("no installed KMP plugin was found in Claude Code or Codex")
            }
            Self::UnsafePath(path) => write!(
                formatter,
                "refusing unsafe lifecycle path `{}`; an absolute non-root path is required",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LifecycleError {}
