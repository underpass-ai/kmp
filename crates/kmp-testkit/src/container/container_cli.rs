use std::ffi::OsStr;
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

use super::ContainerError;

/// Overrides the CLI choice, e.g. `KMP_CONTAINER_CLI=podman`.
const CLI_OVERRIDE_ENV: &str = "KMP_CONTAINER_CLI";

static SELECTED_CLI: OnceLock<Result<String, String>> = OnceLock::new();

/// Checks once per process that a container CLI can reach its engine.
///
/// Docker wins when `docker info` succeeds (it honours `DOCKER_HOST`, so a
/// Podman socket exported by `scripts/ci/testcontainers-runtime.sh` works
/// too); otherwise the `podman` CLI is used directly.
pub fn ensure_container_runtime() -> Result<(), ContainerError> {
    ContainerCli::selected().map(|_| ())
}

/// The container CLI binary and the few commands the harness needs.
#[derive(Debug, Clone)]
pub(super) struct ContainerCli {
    program: String,
}

impl ContainerCli {
    pub(super) fn selected() -> Result<Self, ContainerError> {
        SELECTED_CLI
            .get_or_init(select_program)
            .clone()
            .map(|program| Self { program })
            .map_err(|message| std::io::Error::other(message).into())
    }

    pub(super) fn program(&self) -> &str {
        &self.program
    }

    /// Runs one CLI command and returns its trimmed standard output.
    pub(super) fn run<I, S>(&self, args: I) -> Result<String, ContainerError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args: Vec<_> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect();
        let output = self.output(&args)?;
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "`{} {}` failed ({}): {}",
                self.program,
                args.iter()
                    .map(|arg| arg.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ))
            .into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Runs one CLI command and returns its raw output, success or not.
    pub(super) fn output<I, S>(&self, args: I) -> Result<Output, ContainerError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Ok(Command::new(&self.program)
            .args(args)
            .stdin(Stdio::null())
            .output()?)
    }

    /// Best-effort command whose outcome does not matter (cleanup).
    pub(super) fn run_quietly<I, S>(&self, args: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let _ = Command::new(&self.program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn select_program() -> Result<String, String> {
    if let Some(program) = std::env::var_os(CLI_OVERRIDE_ENV) {
        let program = program.to_string_lossy().into_owned();
        return if engine_answers(&program) {
            Ok(program)
        } else {
            Err(format!(
                "{CLI_OVERRIDE_ENV}={program} is set but `{program} info` failed"
            ))
        };
    }
    ["docker", "podman"]
        .into_iter()
        .find(|program| engine_answers(program))
        .map(str::to_string)
        .ok_or_else(|| {
            "no container engine: neither `docker info` nor `podman info` succeeded".to_string()
        })
}

fn engine_answers(program: &str) -> bool {
    Command::new(program)
        .arg("info")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
