use crate::plugin_notice::adapters::cached_github_latest_release_source::CachedGithubLatestReleaseSource;
use crate::plugin_notice::adapters::json_plugin_manifest_repository::JsonPluginManifestRepository;
use crate::plugin_notice::adapters::plugin_notice_cli_parser::PluginNoticeCliParser;
use crate::plugin_notice::application::dto::plugin_notice_dto::PluginNoticeDto;
use crate::plugin_notice::application::mappers::plugin_notice_command_mapper::PluginNoticeCommandMapper;
use crate::plugin_notice::application::mappers::plugin_notice_mapper::PluginNoticeMapper;
use crate::plugin_notice::application::use_cases::show_plugin_notice::ShowPluginNotice;
use crate::plugin_notice::domain::plugin_notice_error::PluginNoticeError;

/// Native composition root for the non-mutating plugin notice.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativePluginNotice;

impl NativePluginNotice {
    pub fn execute(arguments: &[&str]) -> Result<PluginNoticeDto, PluginNoticeError> {
        let command = PluginNoticeCliParser::parse(arguments)?;
        let request = PluginNoticeCommandMapper::to_domain(command)?;
        let manifests = JsonPluginManifestRepository;
        let releases = CachedGithubLatestReleaseSource::from_environment();
        let notice = ShowPluginNotice::new(&manifests, &releases).execute(&request)?;
        Ok(PluginNoticeMapper::to_dto(&notice))
    }
}
