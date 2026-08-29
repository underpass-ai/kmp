use serde::Serialize;

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
}
