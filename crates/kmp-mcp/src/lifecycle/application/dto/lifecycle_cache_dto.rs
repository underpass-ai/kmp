use serde::Serialize;

/// One host's superseded plugin-cache releases, after a proved convergence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LifecycleCacheDto {
    pub host: String,
    /// Superseded and still on disk. A session that was already open keeps
    /// reading its skills from one of these until it restarts; the next
    /// process start removes them.
    pub deferred: Vec<String>,
}
