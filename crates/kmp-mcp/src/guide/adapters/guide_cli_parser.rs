use std::path::PathBuf;

use crate::guide::application::dto::guide_sync_request_dto::GuideSyncRequestDto;
use crate::guide::domain::guide_error::GuideError;
use crate::guide::domain::guide_plugin_root::GuidePluginRoot;
use crate::guide::domain::guide_sync_mode::GuideSyncMode;

pub struct GuideCliParser;

impl GuideCliParser {
    pub fn parse(arguments: &[&str]) -> Result<GuideSyncRequestDto, GuideError> {
        if arguments.first().copied() != Some("sync") {
            return Err(GuideError::invalid(
                "usage: kmp-mcp guide sync --plugin-root DIR [--dry-run]",
            ));
        }
        let mut plugin_root = None;
        let mut mode = GuideSyncMode::Apply;
        let mut index = 1;
        while index < arguments.len() {
            match arguments[index] {
                "--plugin-root" => {
                    index += 1;
                    plugin_root =
                        Some(PathBuf::from(arguments.get(index).ok_or_else(|| {
                            GuideError::invalid("--plugin-root needs a directory")
                        })?));
                }
                "--dry-run" => mode = GuideSyncMode::DryRun,
                option => {
                    return Err(GuideError::invalid(format!(
                        "unknown guide option `{option}`"
                    )));
                }
            }
            index += 1;
        }
        let plugin_root =
            plugin_root.ok_or_else(|| GuideError::invalid("guide sync needs --plugin-root DIR"))?;
        Ok(GuideSyncRequestDto::new(
            GuidePluginRoot::parse(plugin_root)?,
            mode,
        ))
    }
}
