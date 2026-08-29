use crate::lifecycle::application::dto::lifecycle_failure_dto::LifecycleFailureDto;
use crate::lifecycle::domain::lifecycle_action::LifecycleAction;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;

/// Maps typed domain failures to the stable CLI error receipt.
#[derive(Clone, Copy, Debug, Default)]
pub struct LifecycleFailureMapper;

impl LifecycleFailureMapper {
    pub fn to_dto(action: LifecycleAction, error: &LifecycleError) -> LifecycleFailureDto {
        LifecycleFailureDto {
            action: Self::action(action).to_string(),
            status: "failed".to_string(),
            failed_component: Self::component(error).to_string(),
            detail: error.to_string(),
        }
    }

    fn action(action: LifecycleAction) -> &'static str {
        match action {
            LifecycleAction::Setup => "setup",
            LifecycleAction::Update => "update",
        }
    }

    fn component(error: &LifecycleError) -> &str {
        match error {
            LifecycleError::CommandFailed { program, .. } => program,
            LifecycleError::HostNotInstalled(_)
            | LifecycleError::HostVersionMismatch(_)
            | LifecycleError::NoInstalledHost => "host_inventory",
            LifecycleError::InvalidCommand(_) => "request",
            LifecycleError::InvalidHostResponse(_) => "host_contract",
            LifecycleError::InvalidReleaseVersion(_)
            | LifecycleError::Network(_)
            | LifecycleError::UnsupportedPlatform(_) => "release",
            LifecycleError::Io { .. } | LifecycleError::UnsafePath(_) => "filesystem",
            LifecycleError::SurfaceMismatch(_) => "engine_surface",
            LifecycleError::TreeMismatch(_) => "plugin_tree",
        }
    }
}
