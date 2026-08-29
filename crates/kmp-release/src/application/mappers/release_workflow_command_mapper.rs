use crate::application::dto::release_workflow_command_dto::ReleaseWorkflowCommandDto;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::domain::workflow_run_id::WorkflowRunId;

pub struct ReleaseWorkflowCommandMapper;

impl ReleaseWorkflowCommandMapper {
    pub const USAGE: &'static str = "usage: kmp-release workflow version VERSION\n       kmp-release workflow candidate VERSION [RUN_ID]\n       kmp-release workflow release VERSION";

    pub fn map(arguments: &[String]) -> Result<ReleaseWorkflowCommandDto, ReleaseError> {
        let action = arguments.first().map(String::as_str).unwrap_or_default();
        let version = arguments
            .get(1)
            .ok_or_else(|| ReleaseError::invalid(Self::USAGE))
            .and_then(|value| ReleaseVersion::parse(value.clone()))?;
        match (action, arguments.len()) {
            ("version", 2) => Ok(ReleaseWorkflowCommandDto::Version { version }),
            ("candidate", 2) => Ok(ReleaseWorkflowCommandDto::Candidate {
                version,
                run_id: None,
            }),
            ("candidate", 3) => Ok(ReleaseWorkflowCommandDto::Candidate {
                version,
                run_id: Some(WorkflowRunId::parse(arguments[2].clone())?),
            }),
            ("release", 2) => Ok(ReleaseWorkflowCommandDto::Release { version }),
            _ => Err(ReleaseError::invalid(Self::USAGE)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_accepts_one_numeric_run_id() {
        let command = ReleaseWorkflowCommandMapper::map(&[
            "candidate".to_string(),
            "0.5.2".to_string(),
            "33234243966".to_string(),
        ])
        .expect("candidate command");

        assert!(matches!(
            command,
            ReleaseWorkflowCommandDto::Candidate {
                run_id: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn workflow_rejects_shell_shaped_extra_arguments() {
        let error = ReleaseWorkflowCommandMapper::map(&[
            "release".to_string(),
            "0.5.2".to_string(),
            "--force".to_string(),
        ])
        .expect_err("extra option must fail");

        assert!(error.to_string().contains("usage:"));
    }
}
