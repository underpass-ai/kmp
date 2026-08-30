use std::fmt::{Display, Formatter};

use crate::domain::readiness_outcome::ReadinessOutcome;

/// One named release readiness check and what it concluded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadinessCheck {
    name: String,
    outcome: ReadinessOutcome,
}

impl ReadinessCheck {
    pub fn passed(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            outcome: ReadinessOutcome::Passed(detail.into()),
        }
    }

    pub fn failed(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            outcome: ReadinessOutcome::Failed(reason.into()),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_failure(&self) -> bool {
        self.outcome.is_failure()
    }

    pub fn detail(&self) -> &str {
        self.outcome.detail()
    }
}

impl Display for ReadinessCheck {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let marker = if self.is_failure() { "FAIL" } else { "ok  " };
        let detail = self.detail();
        let indented = detail.replace('\n', "\n        ");
        write!(formatter, "  {marker}  {}: {indented}", self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_line_reason_stays_inside_its_check() {
        let check = ReadinessCheck::failed("version sources", "one is wrong\nso is another");

        assert_eq!(
            check.to_string(),
            "  FAIL  version sources: one is wrong\n        so is another"
        );
    }
}
