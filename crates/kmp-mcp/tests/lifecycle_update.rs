#[path = "lifecycle_support/fake_engine_store.rs"]
mod fake_engine_store;
#[path = "lifecycle_support/fake_host_gateway.rs"]
mod fake_host_gateway;
#[path = "lifecycle_support/fake_release_repository.rs"]
mod fake_release_repository;

use std::collections::BTreeSet;
use std::path::PathBuf;

use fake_engine_store::FakeEngineStore;
use fake_host_gateway::FakeHostGateway;
use fake_release_repository::FakeReleaseRepository;
use kmp_mcp::lifecycle::SetupKmp;
use kmp_mcp::lifecycle::UpdateKmp;
use kmp_mcp::lifecycle::domain::convergence_status::ConvergenceStatus;
use kmp_mcp::lifecycle::domain::engine_artifact::EngineArtifact;
use kmp_mcp::lifecycle::domain::engine_install_dir::EngineInstallDir;
use kmp_mcp::lifecycle::domain::host::Host;
use kmp_mcp::lifecycle::domain::host_installation::HostInstallation;
use kmp_mcp::lifecycle::domain::lifecycle_action::LifecycleAction;
use kmp_mcp::lifecycle::domain::lifecycle_error::LifecycleError;
use kmp_mcp::lifecycle::domain::lifecycle_request::LifecycleRequest;
use kmp_mcp::lifecycle::domain::plugin_root::PluginRoot;
use kmp_mcp::lifecycle::domain::release_version::ReleaseVersion;

fn version(value: &str) -> ReleaseVersion {
    ReleaseVersion::parse(value).expect("release version")
}

fn installation(host: Host, release: &str, root: &str) -> HostInstallation {
    HostInstallation::discovered(
        host,
        version(release),
        PluginRoot::new(root).expect("plugin root"),
        true,
    )
}

fn request(
    action: LifecycleAction,
    hosts: BTreeSet<Host>,
    target: Option<ReleaseVersion>,
) -> LifecycleRequest {
    LifecycleRequest::new(
        action,
        hosts,
        target,
        EngineInstallDir::new("/tmp/shared").expect("shared engine dir"),
        false,
    )
}

#[test]
fn update_from_0_4_2_converges_claude_codex_and_the_shared_engine() {
    let hosts = FakeHostGateway::with_installations(vec![
        installation(Host::Claude, "0.4.2", "/tmp/claude"),
        installation(Host::Codex, "0.4.2", "/tmp/codex"),
    ]);
    let target = version("0.5.2");
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty();
    let selected = BTreeSet::from([Host::Codex]);

    let receipt = UpdateKmp::new(&hosts, &releases, &engines)
        .execute(request(
            LifecycleAction::Update,
            selected,
            Some(target.clone()),
        ))
        .expect("converged update");

    assert_eq!(hosts.refreshes(), vec![Host::Claude, Host::Codex]);
    assert_eq!(
        engines.installations(),
        vec![
            PathBuf::from("/tmp/claude/bin"),
            PathBuf::from("/tmp/shared"),
        ]
    );
    assert_eq!(receipt.version(), &target);
    assert_eq!(receipt.hosts().len(), 2);
    assert_eq!(receipt.engine_proofs().len(), 2);
    assert!(receipt.plugin_tree().is_some());
    assert_eq!(engines.staged_count(), 1);
}

#[test]
fn setup_of_current_plugins_uses_the_running_release_without_mutating_host_managers() {
    let target = ReleaseVersion::current();
    let hosts = FakeHostGateway::with_installations(vec![
        installation(Host::Claude, target.as_str(), "/tmp/claude"),
        installation(Host::Codex, target.as_str(), "/tmp/codex"),
    ]);
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::running(EngineArtifact::verified(
        target.clone(),
        b"running-engine".to_vec(),
    ));

    let receipt = SetupKmp::new(&hosts, &releases, &engines)
        .execute(request(LifecycleAction::Setup, BTreeSet::new(), None))
        .expect("clean setup");

    assert!(hosts.refreshes().is_empty());
    assert!(hosts.provisions().is_empty());
    assert_eq!(receipt.hosts().len(), 2);
    assert_eq!(receipt.engine_proofs().len(), 2);
}

#[test]
fn clean_setup_provisions_both_native_hosts_from_the_running_binary() {
    let target = ReleaseVersion::current();
    let hosts = FakeHostGateway::with_installations(Vec::new());
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::running(EngineArtifact::verified(
        target.clone(),
        b"running-engine".to_vec(),
    ));

    let receipt = SetupKmp::new(&hosts, &releases, &engines)
        .execute(request(LifecycleAction::Setup, BTreeSet::new(), None))
        .expect("clean native setup");

    assert_eq!(hosts.provisions(), vec![Host::Claude, Host::Codex]);
    assert!(hosts.refreshes().is_empty());
    assert_eq!(receipt.hosts().len(), 2);
    assert_eq!(receipt.engine_proofs().len(), 2);
}

#[test]
fn claude_only_setup_never_mutates_an_unconsumed_shared_engine() {
    let target = ReleaseVersion::current();
    let hosts = FakeHostGateway::with_installations(Vec::new());
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::running(EngineArtifact::verified(
        target.clone(),
        b"running-engine".to_vec(),
    ));

    let receipt = SetupKmp::new(&hosts, &releases, &engines)
        .execute(request(
            LifecycleAction::Setup,
            BTreeSet::from([Host::Claude]),
            None,
        ))
        .expect("Claude setup");

    assert_eq!(hosts.provisions(), vec![Host::Claude]);
    assert_eq!(
        engines.installations(),
        vec![PathBuf::from("/tmp/claude/bin")]
    );
    assert_eq!(receipt.engine_proofs().len(), 1);
    assert_eq!(receipt.engine_proofs()[0].host(), Host::Claude);
}

#[test]
fn update_rejects_a_host_that_did_not_reach_the_requested_release() {
    let hosts = FakeHostGateway::with_installations(vec![installation(
        Host::Claude,
        "0.4.2",
        "/tmp/claude",
    )])
    .returning_version(version("0.5.1"));
    let target = version("0.5.2");
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty();

    let error = UpdateKmp::new(&hosts, &releases, &engines)
        .execute(request(
            LifecycleAction::Update,
            BTreeSet::new(),
            Some(target),
        ))
        .expect_err("stale host must fail");

    assert!(matches!(error, LifecycleError::HostVersionMismatch(_)));
    assert!(engines.installations().is_empty());
}

#[test]
fn update_rejects_non_identical_codex_and_claude_plugin_trees() {
    let target = version("0.5.2");
    let hosts = FakeHostGateway::with_installations(vec![
        installation(Host::Claude, "0.4.2", "/tmp/claude"),
        installation(Host::Codex, "0.4.2", "/tmp/codex"),
    ]);
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty().with_divergent_trees();

    let error = UpdateKmp::new(&hosts, &releases, &engines)
        .execute(request(
            LifecycleAction::Update,
            BTreeSet::new(),
            Some(target),
        ))
        .expect_err("different plugin trees must fail");

    assert!(matches!(error, LifecycleError::TreeMismatch(_)));
    assert_eq!(
        engines.installations(),
        vec![PathBuf::from("/tmp/claude/bin")]
    );
}

#[test]
fn update_proves_the_release_before_mutating_any_host() {
    let target = version("0.5.2");
    let hosts = FakeHostGateway::with_installations(vec![installation(
        Host::Claude,
        "0.4.2",
        "/tmp/claude",
    )]);
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty().with_rejected_stage();

    let error = UpdateKmp::new(&hosts, &releases, &engines)
        .execute(request(
            LifecycleAction::Update,
            BTreeSet::new(),
            Some(target),
        ))
        .expect_err("unproved release must fail before host refresh");

    assert!(matches!(error, LifecycleError::SurfaceMismatch(_)));
    assert_eq!(engines.staged_count(), 1);
    assert!(hosts.refreshes().is_empty());
    assert!(engines.installations().is_empty());
}

#[test]
fn dry_run_distinguishes_the_observed_release_from_the_planned_target() {
    let target = version("0.5.2");
    let hosts = FakeHostGateway::with_installations(vec![installation(
        Host::Claude,
        "0.4.2",
        "/tmp/claude",
    )]);
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty();
    let dry_run = LifecycleRequest::new(
        LifecycleAction::Update,
        BTreeSet::new(),
        Some(target.clone()),
        EngineInstallDir::new("/tmp/shared").expect("shared engine dir"),
        true,
    );

    let receipt = UpdateKmp::new(&hosts, &releases, &engines)
        .execute(dry_run)
        .expect("planned update");

    assert!(receipt.is_dry_run());
    assert_eq!(receipt.hosts().len(), 1);
    assert_eq!(
        receipt.hosts()[0].status(),
        ConvergenceStatus::PlannedChange
    );
    assert_eq!(
        receipt.hosts()[0]
            .previous_version()
            .expect("observed version")
            .as_str(),
        "0.4.2"
    );
    assert_eq!(receipt.hosts()[0].version(), &target);
    assert!(hosts.refreshes().is_empty());
    assert_eq!(engines.staged_count(), 0);
}

#[test]
fn setup_refreshes_a_disabled_plugin_even_when_its_version_matches() {
    let target = ReleaseVersion::current();
    let disabled = HostInstallation::discovered(
        Host::Claude,
        target.clone(),
        PluginRoot::new("/tmp/claude").expect("plugin root"),
        false,
    );
    let hosts = FakeHostGateway::with_installations(vec![disabled]);
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::running(EngineArtifact::verified(
        target.clone(),
        b"running-engine".to_vec(),
    ));

    let receipt = SetupKmp::new(&hosts, &releases, &engines)
        .execute(request(LifecycleAction::Setup, BTreeSet::new(), None))
        .expect("disabled plugin converges");

    assert_eq!(hosts.refreshes(), vec![Host::Claude]);
    assert_eq!(receipt.hosts()[0].status(), ConvergenceStatus::Changed);
    assert!(receipt.hosts()[0].is_enabled());
}
