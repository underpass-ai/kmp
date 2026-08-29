use std::path::PathBuf;
use std::sync::Mutex;

use kmp_mcp::lifecycle::domain::lifecycle_error::LifecycleError;
use kmp_mcp::lifecycle::ports::process_executor::ProcessExecutor;
use kmp_mcp::lifecycle::ports::process_output::ProcessOutput;

pub struct FakeProcessExecutor {
    expected: Mutex<Vec<(String, Vec<String>, ProcessOutput)>>,
}

impl FakeProcessExecutor {
    pub fn expecting(expected: Vec<(String, Vec<String>, ProcessOutput)>) -> Self {
        Self {
            expected: Mutex::new(expected),
        }
    }

    pub fn is_exhausted(&self) -> bool {
        self.expected.lock().expect("process lock").is_empty()
    }
}

impl ProcessExecutor for FakeProcessExecutor {
    fn resolve(&self, program: &str) -> Option<PathBuf> {
        Some(PathBuf::from("/tmp/bin").join(program))
    }

    fn execute(&self, program: &str, arguments: &[&str]) -> Result<ProcessOutput, LifecycleError> {
        let mut expected = self.expected.lock().expect("process lock");
        assert!(
            !expected.is_empty(),
            "unexpected command: {program} {arguments:?}"
        );
        let (expected_program, expected_arguments, output) = expected.remove(0);
        assert_eq!(program, expected_program);
        assert_eq!(
            arguments,
            expected_arguments
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
        Ok(output)
    }
}
