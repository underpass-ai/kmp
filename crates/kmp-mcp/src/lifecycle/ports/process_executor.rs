use std::path::PathBuf;

use super::process_output::ProcessOutput;
use crate::lifecycle::domain::lifecycle_error::LifecycleError;

/// Outbound port for native host process execution.
pub trait ProcessExecutor: Send + Sync {
    fn resolve(&self, program: &str) -> Option<PathBuf>;

    fn is_available(&self, program: &str) -> bool {
        self.resolve(program).is_some()
    }

    fn execute(&self, program: &str, arguments: &[&str]) -> Result<ProcessOutput, LifecycleError>;
}
