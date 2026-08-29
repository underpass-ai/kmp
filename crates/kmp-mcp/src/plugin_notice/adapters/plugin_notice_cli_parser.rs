use std::path::PathBuf;

use crate::plugin_notice::application::dto::plugin_notice_command_dto::PluginNoticeCommandDto;
use crate::plugin_notice::domain::plugin_notice_error::PluginNoticeError;

/// Inbound CLI adapter for `plugin notice`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PluginNoticeCliParser;

impl PluginNoticeCliParser {
    pub fn parse(arguments: &[&str]) -> Result<PluginNoticeCommandDto, PluginNoticeError> {
        if arguments.first() != Some(&"notice") {
            return Err(PluginNoticeError::InvalidCommand(
                "usage: kmp-mcp plugin notice --plugin-root DIR".to_string(),
            ));
        }
        let mut root = None;
        let mut position = 1;
        while position < arguments.len() {
            match arguments[position] {
                "--plugin-root" => {
                    position += 1;
                    root = arguments.get(position).map(PathBuf::from);
                    if root.is_none() {
                        return Err(PluginNoticeError::InvalidCommand(
                            "--plugin-root needs a directory".to_string(),
                        ));
                    }
                }
                option => {
                    return Err(PluginNoticeError::InvalidCommand(format!(
                        "unknown notice option `{option}`"
                    )));
                }
            }
            position += 1;
        }
        Ok(PluginNoticeCommandDto {
            plugin_root: root.ok_or_else(|| {
                PluginNoticeError::InvalidCommand(
                    "plugin notice needs --plugin-root DIR".to_string(),
                )
            })?,
        })
    }
}
