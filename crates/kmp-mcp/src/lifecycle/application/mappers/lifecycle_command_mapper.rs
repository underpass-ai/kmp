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
                "hermes" => Ok(Host::Hermes),
                "pi" => Ok(Host::Pi),
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn command(hosts: &[&str]) -> LifecycleCommandDto {
        LifecycleCommandDto {
            hosts: hosts.iter().map(ToString::to_string).collect(),
            version: None,
            install_dir: PathBuf::from("/tmp/kmp-bin"),
            dry_run: true,
            lexical_bridge: None,
            decline_bridge: true,
            bridge_dir: None,
        }
    }

    #[test]
    fn pi_is_a_host_the_domain_recognizes() {
        let request = LifecycleCommandMapper::to_domain(command(&["pi"]), LifecycleAction::Setup)
            .expect("request");

        assert_eq!(
            request
                .requested_hosts()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![Host::Pi]
        );
    }

    #[test]
    fn an_unknown_host_is_refused_at_the_boundary() {
        assert!(
            LifecycleCommandMapper::to_domain(command(&["cursor"]), LifecycleAction::Setup)
                .is_err()
        );
    }
}
