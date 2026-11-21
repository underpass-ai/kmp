//! What the doctor is allowed to claim about a host (#680).
//!
//! Three different facts were being reported as one: that a registration is
//! installed and enabled, that the engine binary declares the tool surface,
//! and that something actually connected. The bundled doctor called a Codex
//! registration "usable" while no conversation had a single KMP tool. These
//! tests hold each fact to its own sentence and its own severity.

// The shared fakes serve setup, update and diagnosis; this binary only needs
// the part of them a read-only diagnosis touches.
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

/// #849: the doctor compares the same trees the convergence compares. A
/// Hermes home is not a marketplace plugin tree, so identical Claude and Codex
/// trees are reported identical while Hermes is installed beside them. The
/// same holds for a Pi agent home.
#[test]
fn hermes_beside_identical_plugin_trees_is_not_a_parity_failure() {
    let hosts = FakeHostGateway::with_installations(vec![
        installed(Host::Claude, "/tmp/claude"),
        installed(Host::Codex, "/tmp/codex"),
        installed(Host::Hermes, "/tmp/hermes"),
        installed(Host::Pi, "/tmp/pi"),
    ]);
    let engines = FakeEngineStore::empty();
    let findings = DiagnoseLifecycle::new(&hosts, &engines)
        .execute()
        .findings()
        .to_vec();

    let parity = one(
        &findings,
        "Claude Code and Codex plugin trees are byte-for-byte identical",
    );
    assert_eq!(parity.severity(), DiagnosticSeverity::Ok);
    assert!(
        !findings
            .iter()
            .any(|finding| finding.headline().contains("plugin trees differ")),
        "Hermes produced a false parity failure: {:?}",
        findings
            .iter()
            .map(LifecycleFinding::headline)
            .collect::<Vec<_>>()
    );
}

/// A real divergence still fails, and says which hosts disagree.
#[test]
fn a_parity_failure_names_the_hosts_whose_trees_differ() {
    let hosts = FakeHostGateway::with_installations(vec![
        installed(Host::Claude, "/tmp/claude"),
        installed(Host::Codex, "/tmp/codex"),
        installed(Host::Hermes, "/tmp/hermes"),
        installed(Host::Pi, "/tmp/pi"),
    ]);
    let engines = FakeEngineStore::empty().with_divergent_trees();
    let findings = DiagnoseLifecycle::new(&hosts, &engines)
        .execute()
        .findings()
        .to_vec();

    let parity = one(&findings, "Claude Code and Codex plugin trees differ");
    assert_eq!(parity.severity(), DiagnosticSeverity::Fail);
    assert!(says(&parity, "claude"));
    assert!(says(&parity, "codex"));
    assert!(!says(&parity, "hermes"));
}

/// Pi is a peer in the doctor: its pi-runtime package is inventory, never a
/// connection, and its engine is the shared one on PATH.
#[test]
fn pi_is_diagnosed_as_a_peer_whose_registration_is_unverified() {
    let hosts = FakeHostGateway::with_installations(vec![installed(Host::Pi, "/tmp/pi")]);
    let engines = FakeEngineStore::empty();
    let findings = DiagnoseLifecycle::new(&hosts, &engines)
        .execute()
        .findings()
        .to_vec();

    let registration = one(
        &findings,
        "pi: MCP registration installed and enabled, live connection unverified",
    );
    assert_eq!(registration.severity(), DiagnosticSeverity::Warn);
    let declared = one(
        &findings,
        &format!(
            "pi: effective engine binary declares all {} tools",
            kmp_mcp::tool_names().len()
        ),
    );
    assert_eq!(declared.severity(), DiagnosticSeverity::Ok);
    assert!(
        !findings
            .iter()
            .any(|finding| finding.headline().contains("plugin trees differ")),
        "a Pi agent home is not a plugin tree"
    );
}

/// S10: a host whose plugin and engine agree on a release newer than this
/// doctor is warned about, never failed, and never told to converge back.
#[test]
fn an_older_doctor_warns_about_a_newer_consistent_install_instead_of_failing_it() {
    let newer = ReleaseVersion::parse("9.9.9").expect("release");
    let hosts = FakeHostGateway::with_installations(vec![HostInstallation::discovered(
        Host::Claude,
        newer.clone(),
        PluginRoot::new("/tmp/claude").expect("plugin root"),
        true,
    )])
    .reporting(HostRuntimeStatus::Connected);
    let engines = FakeEngineStore::empty().reporting_version(newer);

    let findings = DiagnoseLifecycle::new(&hosts, &engines)
        .execute()
        .findings()
        .to_vec();

    assert!(
        findings
            .iter()
            .all(|finding| finding.severity() != DiagnosticSeverity::Fail),
        "{findings:?}"
    );
    let warning = findings
        .iter()
        .find(|finding| finding.headline().contains("this doctor is the older"))
        .expect("the reader is told this doctor is behind");
    assert_eq!(warning.severity(), DiagnosticSeverity::Warn);
    assert!(says(warning, "doctor`"), "{warning:?}");
    assert!(
        !findings
            .iter()
            .any(|finding| says(finding, "kmp-mcp update")),
        "{findings:?}"
    );
}

/// S11: one host whose inventory cannot be read fails for that host; the
/// others are still diagnosed.
#[test]
fn an_unreadable_host_inventory_is_that_hosts_failure_alone() {
    let hosts = FakeHostGateway::with_installations(vec![installed(Host::Claude, "/tmp/claude")])
        .reporting(HostRuntimeStatus::Connected)
        .with_unreadable_inventory(Host::Hermes, "Hermes MCP config is not a mapping");
    let engines = FakeEngineStore::empty();

    let findings = DiagnoseLifecycle::new(&hosts, &engines)
        .execute()
        .findings()
        .to_vec();

    let hermes = one(&findings, "hermes: plugin inventory failed");
    assert_eq!(hermes.severity(), DiagnosticSeverity::Fail);
    let claude = one(&findings, "claude: live MCP connection verified");
    assert_eq!(claude.severity(), DiagnosticSeverity::Ok);
}

/// S13: a plugin the host lists with nothing behind it is a failure.
#[test]
fn a_hollow_plugin_directory_fails_the_diagnosis() {
    let hosts = FakeHostGateway::with_installations(vec![installed(Host::Claude, "/tmp/claude")])
        .reporting(HostRuntimeStatus::Connected)
        .with_hollow(Host::Claude);
    let engines = FakeEngineStore::empty();

    let findings = DiagnoseLifecycle::new(&hosts, &engines)
        .execute()
        .findings()
        .to_vec();

    let hollow = one(&findings, "claude: installation incomplete on disk");
    assert_eq!(hollow.severity(), DiagnosticSeverity::Fail);
}

/// Doctor sees what `uninstall` sees: a hand-written `kmp` beside the Codex
/// plugin is a second owner, the retired `kernel-memory` id is one wherever it
/// is, and a lone hand-written `kmp` with no plugin is the only owner.
#[test]
fn hand_wiring_beside_a_plugin_and_the_retired_id_are_second_owners() {
    use kmp_mcp::lifecycle::domain::hand_wired_registration::HandWiredRegistration;

    let codex_config = std::path::PathBuf::from("/tmp/codex-home/config.toml");
    let claude_config = std::path::PathBuf::from("/tmp/home/.claude.json");
    let hosts = FakeHostGateway::with_installations(vec![installed(Host::Codex, "/tmp/codex")]);
    let engines = FakeEngineStore::empty();
    let findings = DiagnoseLifecycle::new(&hosts, &engines)
        .with_hand_wiring(vec![
            HandWiredRegistration::new(Host::Codex, "kmp", codex_config.clone()),
            HandWiredRegistration::new(Host::Claude, "kmp", claude_config.clone()),
            HandWiredRegistration::new(Host::Claude, "kernel-memory", claude_config),
        ])
        .execute()
        .findings()
        .to_vec();

    let second = one(
        &findings,
        "codex: a second MCP owner is wired by hand beside the plugin",
    );
    assert_eq!(second.severity(), DiagnosticSeverity::Warn);
    assert!(says(&second, "/tmp/codex-home/config.toml"));
    assert!(says(
        &second,
        "delete the [mcp_servers.kmp] block, then restart codex"
    ));
    let retired = one(
        &findings,
        "claude: a retired `kernel-memory` registration is still wired",
    );
    assert!(says(&retired, "claude mcp remove kernel-memory"));
    // Claude Code has no plugin here, so its own `kmp` is its only owner.
    assert!(
        !findings
            .iter()
            .any(|finding| finding.headline().starts_with("claude: a second MCP owner")),
        "{:?}",
        findings
            .iter()
            .map(LifecycleFinding::headline)
            .collect::<Vec<_>>()
    );
}

/// With nothing installed the retired id is still reported, after the
/// not-installed warning.
#[test]
fn the_retired_id_is_reported_when_no_plugin_is_installed() {
    use kmp_mcp::lifecycle::domain::hand_wired_registration::HandWiredRegistration;

    let hosts = FakeHostGateway::with_installations(Vec::new());
    let engines = FakeEngineStore::empty();
    let findings = DiagnoseLifecycle::new(&hosts, &engines)
        .with_hand_wiring(vec![
            HandWiredRegistration::new(Host::Codex, "kmp", "/tmp/c/config.toml".into()),
            HandWiredRegistration::new(Host::Codex, "kernel-memory", "/tmp/c/config.toml".into()),
        ])
        .execute()
        .findings()
        .to_vec();
    let headlines = findings
        .iter()
        .map(LifecycleFinding::headline)
        .collect::<Vec<_>>();
    assert_eq!(
        headlines,
        vec![
            "KMP is not installed in a native host",
            "codex: a retired `kernel-memory` registration is still wired",
        ]
    );
}
