use crate::plugin_notice::application::dto::plugin_notice_dto::PluginNoticeDto;
use crate::plugin_notice::domain::plugin_notice::PluginNotice;

/// Keeps user-facing hook text out of the domain.
#[derive(Clone, Copy, Debug, Default)]
pub struct PluginNoticeMapper;

impl PluginNoticeMapper {
    pub fn to_dto(notice: &PluginNotice) -> PluginNoticeDto {
        match notice {
            PluginNotice::Quiet => PluginNoticeDto::Quiet,
            PluginNotice::Misaligned { engine, plugin } => PluginNoticeDto::Misaligned {
                engine_version: engine.to_string(),
                plugin_version: plugin.to_string(),
            },
            PluginNotice::UpdateAvailable { installed, latest } => {
                PluginNoticeDto::UpdateAvailable {
                    installed_version: installed.to_string(),
                    latest_version: latest.to_string(),
                }
            }
        }
    }

    pub fn to_text(notice: &PluginNoticeDto) -> Option<String> {
        match notice {
            PluginNoticeDto::Quiet => None,
            PluginNoticeDto::Misaligned {
                engine_version,
                plugin_version,
            } => Some(format!(
                "KMP: engine {engine_version}, plugin {plugin_version}. Run /kmp:setup to align the plugin and engine."
            )),
            PluginNoticeDto::UpdateAvailable {
                installed_version,
                latest_version,
            } => Some(format!(
                "KMP: {installed_version} is installed; {latest_version} is available. Run /kmp:setup to update the plugin and engine together, then restart the session."
            )),
        }
    }
}
