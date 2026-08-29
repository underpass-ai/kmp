use std::path::PathBuf;

/// Raw values accepted at the command boundary.
pub struct PluginNoticeCommandDto {
    pub plugin_root: PathBuf,
}
