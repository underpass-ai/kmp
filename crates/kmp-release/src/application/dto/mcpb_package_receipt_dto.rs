use std::path::PathBuf;

use crate::domain::mcpb_digest::McpbDigest;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct McpbPackageReceiptDto {
    pub archive: PathBuf,
    pub digest: McpbDigest,
}
