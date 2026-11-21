use crate::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use crate::lifecycle::domain::hand_wired_registration::HandWiredRegistration;
use crate::lifecycle::domain::host_connection_verification::HostConnectionVerification;
use crate::lifecycle::domain::host_installation::HostInstallation;
use crate::lifecycle::domain::host_inventory::HostInventory;
use crate::lifecycle::domain::lifecycle_diagnosis::LifecycleDiagnosis;
use crate::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use crate::lifecycle::domain::plugin_tree_parity::PluginTreeParity;
use crate::lifecycle::domain::release_version::ReleaseVersion;
use crate::lifecycle::ports::engine_store::EngineStore;
use crate::lifecycle::ports::host_gateway::HostGateway;

/// Use case: diagnose plugin, engine and effective MCP state without mutating
/// any host installation or selected memory.
pub struct DiagnoseLifecycle<'a> {
    hosts: &'a dyn HostGateway,
    engines: &'a dyn EngineStore,
    hand_wiring: Vec<HandWiredRegistration>,
}

impl<'a> DiagnoseLifecycle<'a> {
    pub fn new(hosts: &'a dyn HostGateway, engines: &'a dyn EngineStore) -> Self {
        Self {
            hosts,
            engines,
            hand_wiring: Vec::new(),
        }
    }

    /// The registrations written by hand into the hosts' own configuration
    /// files — the same survey `uninstall` lists — judged beside the plugins
    /// the hosts report.
    pub fn with_hand_wiring(mut self, registrations: Vec<HandWiredRegistration>) -> Self {
        self.hand_wiring = registrations;
        self
    }

    pub fn execute(&self) -> LifecycleDiagnosis {
        let inventory = self.hosts.inventory();
        let mut findings = self.host_findings(&inventory);
        findings.extend(self.hand_wiring_findings(inventory.installed()));
        LifecycleDiagnosis::from_findings(findings)
    }

    /// A second owner beside a plugin, and the retired id anywhere. A lone
    /// hand-written `kmp` with no plugin is the only owner, not a second one.
    fn hand_wiring_findings(&self, installed: &[HostInstallation]) -> Vec<LifecycleFinding> {
        self.hand_wiring
            .iter()
            .filter_map(|registration| {
                let host = registration.host();
                let (headline, why) = if registration.is_retired() {
                    (
                        format!("{host}: a retired `kernel-memory` registration is still wired"),
                        "nothing installs this id any more; it starts its own KMP server, \
                         beside the plugin's when one is installed",
                    )
                } else if installed.iter().any(|installation| {
                    installation.host() == host && installation.participates_in_convergence()
                }) {
                    (
                        format!("{host}: a second MCP owner is wired by hand beside the plugin"),
                        "it starts another KMP server with the same tools, and which one \
                         answers is the host's choice; the plugin is the one owner KMP keeps \
                         current",
                    )
                } else {
                    return None;
                };
                Some(
                    LifecycleFinding::new(DiagnosticSeverity::Warn, headline)
                        .with_detail(registration.path().display().to_string())
                        .with_detail(why)
                        .with_detail(format!("{}, then restart {host}", registration.removal())),
                )
            })
            .collect()
    }

    fn host_findings(&self, inventory: &HostInventory) -> Vec<LifecycleFinding> {
        let installed = inventory.installed();
        // A host whose plugin manager could not be read is reported against
        // that host alone; every other host is still diagnosed (S11).
        let mut findings: Vec<LifecycleFinding> = inventory
            .unreadable()
            .iter()
            .map(|(host, reason)| {
                LifecycleFinding::new(
                    DiagnosticSeverity::Fail,
                    format!("{host}: plugin inventory failed"),
                )
                .with_detail(reason.clone())
                .with_detail("the other hosts were diagnosed without it")
            })
            .collect();
        if installed.is_empty() && !findings.is_empty() {
            return findings;
        }
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
            return findings;
        }

        let target = ReleaseVersion::current();
        for installation in installed {
            findings.extend(self.diagnose_host(installation, &target));
        }
        // The same trees the convergence compares: a Hermes home is never a
        // marketplace plugin tree (#849).
        let trees = installed
            .iter()
            .filter(|installation| installation.participates_in_convergence())
            .filter(|installation| installation.host().installs_plugin_tree())
            .collect::<Vec<_>>();
        if trees.len() > 1 {
            findings.push(self.diagnose_parity(&trees));
        }
        findings
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
        if let Some(newer) = self.newer_installation(installation, target) {
            // An older engine cannot judge a newer tool surface, and telling
            // a healthy host to converge backwards is the one wrong answer.
            findings.push(newer);
            findings.push(self.registration_finding(host));
            return findings;
        }
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

        if let Err(error) = self.hosts.verify_on_disk(installation, target) {
            // A plugin manager's entry with nothing behind it is broken; a
            // host without one that lacks skills still serves memory.
            let severity = if host.installs_plugin_tree() {
                DiagnosticSeverity::Fail
            } else {
                DiagnosticSeverity::Warn
            };
            findings.push(
                LifecycleFinding::new(severity, format!("{host}: installation incomplete on disk"))
                    .with_detail(error.to_string()),
            );
        }

        match self
            .hosts
            .runtime_engine(installation)
            .and_then(|engine| self.engines.prove(&engine, target))
        {
            // What this proves is the executable's own surface. It is run
            // directly, against a scratch directory, with no host in the
            // loop — so it is reported as a declaration, not a connection.
            Ok(proof) => findings.push(
                LifecycleFinding::new(
                    DiagnosticSeverity::Ok,
                    format!(
                        "{host}: effective engine binary declares all {} tools",
                        proof.tool_count()
                    ),
                )
                .with_detail(proof.executable().as_path().display().to_string())
                .with_detail(
                    "proved by running this executable directly; it is not evidence of a host \
                     connection",
                ),
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

        findings.push(self.registration_finding(host));
        findings
    }

    /// The warning for an installation newer than this engine whose own
    /// engine reports that same newer release: the host agrees with itself,
    /// and it is this doctor that is behind (S10). None when the plugin is
    /// not newer, or when its engine says something else — that is still a
    /// mismatch to fail on.
    fn newer_installation(
        &self,
        installation: &HostInstallation,
        target: &ReleaseVersion,
    ) -> Option<LifecycleFinding> {
        let installed = installation.version();
        if !installed.is_newer_than(target) {
            return None;
        }
        let engine = self.hosts.runtime_engine(installation).ok()?;
        let reported = self.engines.reported_version(&engine).ok()?;
        if !reported.represents_same_release(installed) {
            return None;
        }
        Some(
            LifecycleFinding::new(
                DiagnosticSeverity::Warn,
                format!(
                    "{}: KMP {installed} is installed; this doctor is the older {target}",
                    installation.host()
                ),
            )
            .with_detail(format!(
                "the plugin and its engine agree on {installed}; this older engine cannot judge \
                 their tools"
            ))
            .with_detail(format!(
                "run the installed engine's own diagnosis: `{} doctor`",
                engine.as_path().display()
            )),
        )
    }

    fn registration_finding(&self, host: crate::lifecycle::domain::host::Host) -> LifecycleFinding {
        match self.hosts.runtime_status(host) {
            Ok(status) => {
                let verification = status.verification();
                let severity = match verification {
                    HostConnectionVerification::Verified => DiagnosticSeverity::Ok,
                    // Unverified is a warning, not an approval: the reader
                    // has to know nothing connected (#680).
                    HostConnectionVerification::Unverified => DiagnosticSeverity::Warn,
                    HostConnectionVerification::Broken => DiagnosticSeverity::Fail,
                };
                let mut finding =
                    LifecycleFinding::new(severity, format!("{host}: {}", verification.headline()))
                        .with_detail(status.description());
                if verification == HostConnectionVerification::Unverified {
                    finding = finding.with_detail(
                        "no live MCP probe ran here; ask this host for a kmp tool to verify the \
                         connection, and check the selected memory with `kmp-mcp config`",
                    );
                }
                finding
            }
            Err(error) => LifecycleFinding::new(
                DiagnosticSeverity::Fail,
                format!("{host}: effective MCP registration could not be checked"),
            )
            .with_detail(error.to_string()),
        }
    }

    fn diagnose_parity(&self, installations: &[&HostInstallation]) -> LifecycleFinding {
        let digests = installations
            .iter()
            .map(|installation| {
                self.engines
                    .digest_tree(installation.root())
                    .map(|digest| (installation.host(), digest))
            })
            .collect::<Result<Vec<_>, _>>();
        match digests.map(PluginTreeParity::of) {
            Ok(PluginTreeParity::Identical(digest)) => LifecycleFinding::new(
                DiagnosticSeverity::Ok,
                "Claude Code and Codex plugin trees are byte-for-byte identical",
            )
            .with_detail(digest.to_string()),
            Ok(PluginTreeParity::Unobserved) => LifecycleFinding::new(
                DiagnosticSeverity::Warn,
                "no native plugin tree was observed to compare",
            ),
            Ok(divergent @ PluginTreeParity::Divergent(_)) => LifecycleFinding::new(
                DiagnosticSeverity::Fail,
                "Claude Code and Codex plugin trees differ",
            )
            .with_detail(format!(
                "{} disagree: {}",
                divergent.hosts(),
                divergent.digests()
            ))
            .with_detail("run `kmp-mcp update` and require one exact marketplace snapshot"),
            Err(error) => LifecycleFinding::new(
                DiagnosticSeverity::Fail,
                "native plugin tree parity could not be proved",
            )
            .with_detail(error.to_string()),
        }
    }
}
