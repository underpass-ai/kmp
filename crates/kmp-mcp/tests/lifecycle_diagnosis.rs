//! What the doctor is allowed to claim about a host (#680).
//!
//! Three different facts were being reported as one: that a registration is
//! installed and enabled, that the engine binary declares the tool surface,
//! and that something actually connected. The bundled doctor called a Codex
//! registration "usable" while no conversation had a single KMP tool. These
//! tests hold each fact to its own sentence and its own severity.

// The shared fakes serve setup, update and diagnosis; this binary only needs
// the part of them a read-only diagnosis touches.
#[allow(dead_code)]
#[path = "lifecycle_support/fake_engine_store.rs"]
mod fake_engine_store;
#[path = "lifecycle_support/fake_host_gateway.rs"]
mod fake_host_gateway;

use fake_engine_store::FakeEngineStore;
use fake_host_gateway::FakeHostGateway;
use kmp_mcp::lifecycle::DiagnoseLifecycle;
use kmp_mcp::lifecycle::domain::diagnostic_severity::DiagnosticSeverity;
use kmp_mcp::lifecycle::domain::host::Host;
use kmp_mcp::lifecycle::domain::host_installation::HostInstallation;
use kmp_mcp::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
use kmp_mcp::lifecycle::domain::lifecycle_finding::LifecycleFinding;
use kmp_mcp::lifecycle::domain::plugin_root::PluginRoot;
use kmp_mcp::lifecycle::domain::release_version::ReleaseVersion;

fn installed(host: Host, root: &str) -> HostInstallation {
    HostInstallation::discovered(
        host,
        ReleaseVersion::current(),
        PluginRoot::new(root).expect("plugin root"),
        true,
    )
}

fn diagnose(status: Option<HostRuntimeStatus>) -> Vec<LifecycleFinding> {
    let installations = vec![
        installed(Host::Claude, "/tmp/claude"),
        installed(Host::Codex, "/tmp/codex"),
    ];
    let mut hosts = FakeHostGateway::with_installations(installations);
    if let Some(status) = status {
        hosts = hosts.reporting(status);
    }
    let engines = FakeEngineStore::empty();
    DiagnoseLifecycle::new(&hosts, &engines)
        .execute()
        .findings()
        .to_vec()
}

fn one(findings: &[LifecycleFinding], headline: &str) -> LifecycleFinding {
    findings
        .iter()
        .find(|finding| finding.headline() == headline)
        .unwrap_or_else(|| {
            panic!(
                "no finding said `{headline}`; the diagnosis said {:?}",
                findings
                    .iter()
                    .map(LifecycleFinding::headline)
                    .collect::<Vec<_>>()
            )
        })
        .clone()
}

fn says(finding: &LifecycleFinding, clause: &str) -> bool {
    finding.detail().iter().any(|line| line.contains(clause))
}

/// A host that reports a live connection is the only one reported as
/// verified, and it is the only one that earns an Ok.
#[test]
fn a_live_connection_is_the_only_verified_state() {
    let findings = diagnose(Some(HostRuntimeStatus::Connected));
    let claude = one(&findings, "claude: live MCP connection verified");
    assert_eq!(claude.severity(), DiagnosticSeverity::Ok);
    assert!(says(&claude, "host reports the MCP connected"));
}

/// The defect: an inventory entry is not a connection. It warns, it says so
/// in the headline, and it never uses the word usable.
#[test]
fn a_registered_but_unconnected_host_is_reported_as_unverified() {
    let findings = diagnose(Some(HostRuntimeStatus::Registered));
    let codex = one(
        &findings,
        "codex: MCP registration installed and enabled, live connection unverified",
    );
    assert_eq!(
        codex.severity(),
        DiagnosticSeverity::Warn,
        "an unverified registration is a warning, not an approval"
    );
    assert!(says(&codex, "rather than a connection"));
    assert!(says(&codex, "no live MCP probe ran here"));
    assert!(says(&codex, "`kmp-mcp config`"));
    assert!(
        !codex.headline().contains("usable"),
        "`{}` still claims usability",
        codex.headline()
    );
}

/// A registration the host cannot use is still a failure, unchanged.
#[test]
fn a_broken_registration_still_fails() {
    for broken in [
        HostRuntimeStatus::Missing,
        HostRuntimeStatus::Disabled,
        HostRuntimeStatus::PendingApproval,
        HostRuntimeStatus::Failed("✘ Failed to connect — CONNECTION_CLOSED".to_string()),
    ] {
        let findings = diagnose(Some(broken.clone()));
        let failed = one(&findings, "codex: effective MCP registration is not usable");
        assert_eq!(
            failed.severity(),
            DiagnosticSeverity::Fail,
            "{broken:?} must fail"
        );
    }
}

/// The engine proof runs the executable, not the host. It is reported as a
/// declaration so no reader mistakes it for a working connection.
#[test]
fn the_tool_surface_is_reported_as_the_binarys_own_declaration() {
    let findings = diagnose(None);
    let declared = one(
        &findings,
        &format!(
            "codex: effective engine binary declares all {} tools",
            kmp_mcp::tool_names().len()
        ),
    );
    assert_eq!(declared.severity(), DiagnosticSeverity::Ok);
    assert!(says(&declared, "it is not evidence of a host connection"));

    // And all three facts are separate lines about the same host, so a reader
    // can tell which one is missing.
    let headlines = findings
        .iter()
        .filter(|finding| finding.headline().starts_with("codex: "))
        .map(LifecycleFinding::headline)
        .collect::<Vec<_>>();
    assert_eq!(
        headlines.len(),
        3,
        "installed/enabled, declared tools and the connection are three facts: {headlines:?}"
    );
}
