use std::collections::BTreeSet;

use crate::lifecycle::application::dto::lifecycle_command_dto::LifecycleCommandDto;
use crate::lifecycle::domain::bridge_choice::BridgeChoice;
use crate::lifecycle::domain::bridge_install_dir::BridgeInstallDir;
use crate::lifecycle::domain::engine_install_dir::EngineInstallDir;
use crate::lifecycle::domain::host::Host;
use crate::lifecycle::domain::lifecycle_action::LifecycleAction;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::lifecycle_request::LifecycleRequest;
use crate::lifecycle::domain::release_version::ReleaseVersion;

/// Maps boundary primitives into validated lifecycle value objects.
#[derive(Clone, Copy, Debug, Default)]
pub struct LifecycleCommandMapper;

impl LifecycleCommandMapper {
    pub fn to_domain(
        dto: LifecycleCommandDto,
        action: LifecycleAction,
    ) -> Result<LifecycleRequest, LifecycleError> {
        let hosts = dto
            .hosts
            .into_iter()
            .map(|host| match host.as_str() {
                "claude" => Ok(Host::Claude),
                "codex" => Ok(Host::Codex),
                _ => Err(LifecycleError::InvalidHostResponse(format!(
                    "unsupported lifecycle host `{host}`"
                ))),
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let version = dto
            .version
            .as_deref()
            .map(ReleaseVersion::parse)
            .transpose()?;
        let bridge = match (dto.decline_bridge, dto.lexical_bridge) {
            (true, _) => BridgeChoice::Declined,
            (false, Some(path)) => BridgeChoice::FromFile(path),
            (false, None) => BridgeChoice::FromRelease,
        };
        let bridge_dir = dto.bridge_dir.map(BridgeInstallDir::new).transpose()?;
        Ok(LifecycleRequest::new(
            action,
            hosts,
            version,
            EngineInstallDir::new(dto.install_dir)?,
            dto.dry_run,
        )
        .with_bridge(bridge, bridge_dir))
    }
}
