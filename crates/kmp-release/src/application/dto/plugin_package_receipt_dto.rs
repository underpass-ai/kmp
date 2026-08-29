use std::path::PathBuf;

use crate::domain::mcpb_digest::McpbDigest;
use crate::domain::plugin_package_version::PluginPackageVersion;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PluginPackageReceiptDto {
    pub archive: PathBuf,
    pub digest: McpbDigest,
    pub version: PluginPackageVersion,
}
