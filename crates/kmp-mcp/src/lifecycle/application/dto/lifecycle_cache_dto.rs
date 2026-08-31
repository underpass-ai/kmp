use serde::Serialize;

/// One host's superseded plugin-cache releases, after a proved convergence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LifecycleCacheDto {
    pub host: String,
    pub removed: Vec<String>,
    /// Superseded, but this machine would not let go of it.
    pub kept: Vec<String>,
}
