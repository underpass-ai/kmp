/// Host-process response crossing the process execution port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

impl ProcessOutput {
    pub fn completed(success: bool, stdout: String, stderr: String) -> Self {
        Self {
            success,
            stdout,
            stderr,
        }
    }

    pub fn require_success(self, program: &str) -> Result<Self, String> {
        if self.success {
            Ok(self)
        } else {
            let detail = if self.stderr.trim().is_empty() {
                self.stdout.trim()
            } else {
                self.stderr.trim()
            };
            Err(format!("{program}: {detail}"))
        }
    }

    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    pub fn diagnostic(&self) -> &str {
        if self.stderr.trim().is_empty() {
            self.stdout.trim()
        } else {
            self.stderr.trim()
        }
    }

    pub fn succeeded(&self) -> bool {
        self.success
    }
}
