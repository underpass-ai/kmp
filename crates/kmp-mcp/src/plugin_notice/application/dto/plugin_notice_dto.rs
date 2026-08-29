/// Stable presentation payload emitted by the notice use case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PluginNoticeDto {
    Quiet,
    Misaligned {
        engine_version: String,
        plugin_version: String,
    },
    UpdateAvailable {
        installed_version: String,
        latest_version: String,
    },
}
