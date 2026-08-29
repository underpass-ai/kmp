use crate::lifecycle::domain::plugin_root::PluginRoot;
use crate::plugin_notice::application::dto::plugin_notice_command_dto::PluginNoticeCommandDto;
use crate::plugin_notice::domain::plugin_notice_error::PluginNoticeError;
use crate::plugin_notice::domain::plugin_notice_request::PluginNoticeRequest;

/// Converts an inbound DTO into validated domain values.
#[derive(Clone, Copy, Debug, Default)]
pub struct PluginNoticeCommandMapper;

impl PluginNoticeCommandMapper {
    pub fn to_domain(
        command: PluginNoticeCommandDto,
    ) -> Result<PluginNoticeRequest, PluginNoticeError> {
        let root = PluginRoot::new(command.plugin_root)
            .map_err(|error| PluginNoticeError::InvalidCommand(error.to_string()))?;
        Ok(PluginNoticeRequest::new(root))
    }
}
