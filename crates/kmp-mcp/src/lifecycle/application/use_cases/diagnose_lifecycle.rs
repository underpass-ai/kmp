use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::lifecycle_diagnosis::LifecycleDiagnosis;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::engine_store::EngineStore;
use crate::lifecycle::ports::host_gateway::HostGateway;

/// Use case: diagnose plugin, engine and effective MCP state without mutating
/// any host installation or selected memory.
pub struct DiagnoseLifecycle<'a> {
    hosts: &'a dyn HostGateway,
    engines: &'a dyn EngineStore,
}

impl<'a> DiagnoseLifecycle<'a> {
    pub fn new(hosts: &'a dyn HostGateway, engines: &'a dyn EngineStore) -> Self {
        Self { hosts, engines }
    }

    pub fn execute(&self) -> LifecycleDiagnosis {
        let installed = match self.hosts.inventory() {
            Ok(installed) => installed,
            Err(error) => {
                return LifecycleDiagnosis::from_findings(vec![
                    LifecycleFinding::new(
                        DiagnosticSeverity::Fail,
                        "native plugin inventory failed",
                    )
                    .with_detail(error.to_string()),
                ]);
            }
        };
        let mut findings = Vec::new();
        if installed.is_empty() {
            let available = self.hosts.available_hosts();
            let detail = if available.is_empty() {
                "neither Claude Code nor Codex is on PATH".to_string()
            } else {
                format!(
                    "available hosts: {}; run `kmp-mcp setup`",
                    available
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            findings.push(
                LifecycleFinding::new(
                    DiagnosticSeverity::Warn,
                    "KMP is not installed in a native host",
                )
                .with_detail(detail),
            );
            return LifecycleDiagnosis::from_findings(findings);
        }

        let target = ReleaseVersion::current();
        for installation in &installed {
            findings.extend(self.diagnose_host(installation, &target));
        }
        let enabled = installed
            .iter()
            .filter(|installation| installation.participates_in_convergence())
            .collect::<Vec<_>>();
        if enabled.len() > 1 {
            findings.push(self.diagnose_parity(&enabled));
        }
        LifecycleDiagnosis::from_findings(findings)
    }

    fn diagnose_host(
        &self,
        installation: &HostInstallation,
        target: &ReleaseVersion,
    ) -> Vec<LifecycleFinding> {
        let host = installation.host();
        if !installation.participates_in_convergence() {
            return vec![
                LifecycleFinding::new(
                    DiagnosticSeverity::Warn,
                    format!("{host}: KMP plugin is disabled"),
                )
                .with_detail(installation.root().as_path().display().to_string()),
            ];
        }

        let mut findings = Vec::new();
        match installation.require_release(target) {
            Ok(()) => findings.push(LifecycleFinding::new(
                DiagnosticSeverity::Ok,
                format!(
                    "{host}: plugin {} matches this engine",
                    installation.version()
                ),
            )),
            Err(error) => findings.push(
                LifecycleFinding::new(
                    DiagnosticSeverity::Fail,
                    format!("{host}: plugin and engine releases differ"),
                )
                .with_detail(error.to_string())
                .with_detail("run `kmp-mcp update` to converge every installed host"),
            ),
        }

        match self
            .hosts
            .runtime_engine(installation)
            .and_then(|engine| self.engines.prove(&engine, target))
        {
            Ok(proof) => findings.push(
                LifecycleFinding::new(
                    DiagnosticSeverity::Ok,
                    format!(
                        "{host}: effective engine answers all {} tools",
                        proof.tool_count()
                    ),
                )
                .with_detail(proof.executable().as_path().display().to_string()),
            ),
            Err(error) => findings.push(
                LifecycleFinding::new(
                    DiagnosticSeverity::Fail,
                    format!("{host}: effective engine failed its runtime proof"),
                )
                .with_detail(error.to_string())
                .with_detail("run `kmp-mcp setup` to replace and prove the engine"),
            ),
        }

        match self.hosts.runtime_status(host) {
            Ok(status) if status.is_usable() => findings.push(
                LifecycleFinding::new(
                    DiagnosticSeverity::Ok,
                    format!("{host}: effective MCP registration is usable"),
                )
                .with_detail(status.description()),
            ),
            Ok(status) => findings.push(
                LifecycleFinding::new(
                    DiagnosticSeverity::Fail,
                    format!("{host}: effective MCP registration is not usable"),
                )
                .with_detail(status.description()),
            ),
            Err(error) => findings.push(
                LifecycleFinding::new(
                    DiagnosticSeverity::Fail,
                    format!("{host}: effective MCP registration could not be checked"),
                )
                .with_detail(error.to_string()),
            ),
        }
        findings
    }

    fn diagnose_parity(&self, installations: &[&HostInstallation]) -> LifecycleFinding {
        let digests = installations
            .iter()
            .map(|installation| self.engines.digest_tree(installation.root()))
            .collect::<Result<Vec<_>, _>>();
        match digests {
            Ok(digests) if digests.windows(2).all(|pair| pair[0] == pair[1]) => {
                LifecycleFinding::new(
                    DiagnosticSeverity::Ok,
                    "Claude Code and Codex plugin trees are byte-for-byte identical",
                )
                .with_detail(digests[0].to_string())
            }
            Ok(_) => LifecycleFinding::new(
                DiagnosticSeverity::Fail,
                "Claude Code and Codex plugin trees differ",
            )
            .with_detail("run `kmp-mcp update` and require one exact marketplace snapshot"),
            Err(error) => LifecycleFinding::new(
                DiagnosticSeverity::Fail,
                "native plugin tree parity could not be proved",
            )
            .with_detail(error.to_string()),
        }
    }
}
