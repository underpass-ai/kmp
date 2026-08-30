use std::fmt::{Display, Formatter};

use crate::domain::readiness_check::ReadinessCheck;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;

/// Every static answer to "is this tree ready to release X.Y.Z?", gathered
/// before anything is built. The whole report is rendered whether or not it
/// passes: a tree with two problems should cost one run, not two.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseReadiness {
    version: ReleaseVersion,
    checks: Vec<ReadinessCheck>,
}

impl ReleaseReadiness {
    pub fn new(version: ReleaseVersion, checks: Vec<ReadinessCheck>) -> Self {
        Self { version, checks }
    }

    pub fn checks(&self) -> &[ReadinessCheck] {
        &self.checks
    }

    pub fn failures(&self) -> Vec<&ReadinessCheck> {
        self.checks
            .iter()
            .filter(|check| check.is_failure())
            .collect()
    }

    pub fn is_ready(&self) -> bool {
        self.failures().is_empty()
    }

    /// The rendered report, or the same report as a failure that stops the
    /// release before a candidate is built.
    pub fn into_result(self) -> Result<String, ReleaseError> {
        if self.is_ready() {
            Ok(self.to_string())
        } else {
            Err(ReleaseError::invalid(self.to_string()))
        }
    }
}

impl Display for ReleaseReadiness {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "release preflight {}:", self.version)?;
        for check in &self.checks {
            writeln!(formatter, "{check}")?;
        }
        let failures = self.failures().len();
        if failures == 0 {
            write!(
                formatter,
                "  {} checks passed; this tree can build a {} candidate",
                self.checks.len(),
                self.version
            )
        } else {
            write!(
                formatter,
                "  {failures} of {} checks failed; fix them before building a {} candidate",
                self.checks.len(),
                self.version
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version() -> ReleaseVersion {
        ReleaseVersion::parse("0.6.1").expect("version")
    }

    #[test]
    fn every_failure_is_reported_not_only_the_first() {
        let readiness = ReleaseReadiness::new(
            version(),
            vec![
                ReadinessCheck::failed("version sources", "catalog ref is v0.6.0"),
                ReadinessCheck::passed("working tree", "clean"),
                ReadinessCheck::failed("changelog", "[0.6.1] is empty"),
            ],
        );

        assert_eq!(readiness.failures().len(), 2);
        let report = readiness.clone().into_result().expect_err("not ready");
        assert!(report.to_string().contains("catalog ref is v0.6.0"));
        assert!(report.to_string().contains("[0.6.1] is empty"));
        assert!(report.to_string().contains("2 of 3 checks failed"));
    }

    #[test]
    fn a_ready_tree_returns_its_report() {
        let readiness = ReleaseReadiness::new(
            version(),
            vec![ReadinessCheck::passed("changelog", "ready")],
        );

        let report = readiness.into_result().expect("ready");
        assert!(report.contains("1 checks passed"));
    }
}
