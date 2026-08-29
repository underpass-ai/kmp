use serde::Serialize;

/// Stable output projection for one converged native host.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LifecycleHostDto {
    pub host: String,
    pub status: String,
    pub previous_version: Option<String>,
    pub version: String,
    pub root: Option<String>,
    pub enabled: bool,
}
