//! In-crate test support for the lifecycle. Compiled only under `cfg(test)`
//! so unit tests inside the crate can share the fakes the integration tests
//! mirror in `tests/lifecycle_support/`.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::lifecycle::domain::lifecycle_error::LifecycleError;
use crate::lifecycle::ports::process_executor::ProcessExecutor;
use crate::lifecycle::ports::process_output::ProcessOutput;

pub struct FakeProcessExecutor {
    expected: Mutex<Vec<(String, Vec<String>, ProcessOutput)>>,
    resolved: Mutex<Vec<String>>,
}

impl FakeProcessExecutor {
    pub fn expecting(expected: Vec<(String, Vec<String>, ProcessOutput)>) -> Self {
        Self {
            expected: Mutex::new(expected),
            resolved: Mutex::new(Vec::new()),
        }
    }

    pub fn is_exhausted(&self) -> bool {
        self.expected.lock().expect("process lock").is_empty()
    }
}

impl ProcessExecutor for FakeProcessExecutor {
    fn resolve(&self, program: &str) -> Option<PathBuf> {
        self.resolved
            .lock()
            .expect("resolve lock")
            .push(program.to_string());
        Some(PathBuf::from("/tmp/bin").join(program))
    }

    fn execute(&self, program: &str, arguments: &[&str]) -> Result<ProcessOutput, LifecycleError> {
        let mut expected = self.expected.lock().expect("process lock");
        let position = expected.iter().position(|(name, args, _)| {
            name == program && args.iter().map(String::as_str).collect::<Vec<_>>() == arguments
        });
        let Some(position) = position else {
            return Err(LifecycleError::CommandFailed {
                program: program.to_string(),
                detail: format!("unexpected command: {program} {arguments:?}"),
            });
        };
        Ok(expected.remove(position).2)
    }
}
