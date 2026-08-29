use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::lifecycle::adapters::bounded_child::BoundedChild;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::domain::process_timeout::ProcessTimeout;
use crate::lifecycle::ports::process_executor::ProcessExecutor;
use crate::lifecycle::ports::process_output::ProcessOutput;

/// Native process adapter. Commands are executed directly, never through a
/// shell or a reconstructed command line.
#[derive(Clone, Copy, Debug)]
pub struct SystemProcessExecutor {
    timeout: ProcessTimeout,
}

impl SystemProcessExecutor {
    pub fn new(timeout: ProcessTimeout) -> Self {
        Self { timeout }
    }
}

impl Default for SystemProcessExecutor {
    fn default() -> Self {
        Self::new(ProcessTimeout::default())
    }
}

impl ProcessExecutor for SystemProcessExecutor {
    fn resolve(&self, program: &str) -> Option<PathBuf> {
        let candidate = Path::new(program);
        if candidate.components().count() > 1 {
            return candidate.is_file().then(|| candidate.to_path_buf());
        }
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path).find_map(|directory| {
                let plain = directory.join(program);
                if plain.is_file() {
                    Some(plain)
                } else if cfg!(windows) {
                    let executable = directory.join(format!("{program}.exe"));
                    executable.is_file().then_some(executable)
                } else {
                    None
                }
            })
        })
    }

    fn execute(&self, program: &str, arguments: &[&str]) -> Result<ProcessOutput, LifecycleError> {
        let child = Command::new(program)
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| LifecycleError::CommandFailed {
                program: program.to_string(),
                detail: error.to_string(),
            })?;

        let (output, timed_out) = BoundedChild::wait(child, self.timeout).map_err(|error| {
            LifecycleError::CommandFailed {
                program: program.to_string(),
                detail: error.to_string(),
            }
        })?;
        if timed_out {
            return Err(LifecycleError::CommandFailed {
                program: program.to_string(),
                detail: format!(
                    "timed out after {} seconds: {}",
                    self.timeout.duration().as_secs(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            });
        }
        Ok(ProcessOutput::completed(
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}
