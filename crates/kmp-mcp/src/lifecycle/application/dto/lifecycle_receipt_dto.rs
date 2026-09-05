use serde::Serialize;

use super::lifecycle_bridge_dto::LifecycleBridgeDto;
use super::lifecycle_cache_dto::LifecycleCacheDto;
use super::lifecycle_engine_dto::LifecycleEngineDto;
use super::lifecycle_host_dto::LifecycleHostDto;

/// Machine-readable lifecycle output; domain objects never serialize their
/// internal shape directly at the CLI boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LifecycleReceiptDto {
    pub action: String,
    pub status: String,
    pub version: String,
    pub dry_run: bool,
    pub hosts: Vec<LifecycleHostDto>,
    pub engines: Vec<LifecycleEngineDto>,
    pub plugin_tree_digest: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub plugin_caches: Vec<LifecycleCacheDto>,
    /// Absent on a dry run, which has not touched the table yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lexical_bridge: Option<LifecycleBridgeDto>,
}
