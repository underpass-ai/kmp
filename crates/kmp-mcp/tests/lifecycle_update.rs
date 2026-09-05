#[path = "lifecycle_support/fake_bridge_store.rs"]
mod fake_bridge_store;
#[path = "lifecycle_support/fake_engine_store.rs"]
mod fake_engine_store;
#[path = "lifecycle_support/fake_host_gateway.rs"]
mod fake_host_gateway;
#[path = "lifecycle_support/fake_plugin_cache.rs"]
mod fake_plugin_cache;
#[path = "lifecycle_support/fake_release_repository.rs"]
mod fake_release_repository;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fake_bridge_store::FakeBridgeStore;
use fake_engine_store::FakeEngineStore;
use fake_host_gateway::FakeHostGateway;
use fake_plugin_cache::FakePluginCache;
use fake_release_repository::FakeReleaseRepository;
use kmp_mcp::lifecycle::SetupKmp;
use kmp_mcp::lifecycle::UpdateKmp;
use kmp_mcp::lifecycle::domain::bridge_choice::BridgeChoice;
use kmp_mcp::lifecycle::domain::bridge_install_dir::BridgeInstallDir;
use kmp_mcp::lifecycle::domain::bridge_installation::BridgeInstallation;
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

    let receipt = UpdateKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &FakeBridgeStore::default(),
    )
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

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &FakeBridgeStore::default(),
    )
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

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &FakeBridgeStore::default(),
    )
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

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &FakeBridgeStore::default(),
    )
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

    let error = UpdateKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &FakeBridgeStore::default(),
    )
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

    let error = UpdateKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &FakeBridgeStore::default(),
    )
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

    let error = UpdateKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &FakeBridgeStore::default(),
    )
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

    let receipt = UpdateKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &FakeBridgeStore::default(),
    )
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

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &FakeBridgeStore::default(),
    )
    .execute(request(LifecycleAction::Setup, BTreeSet::new(), None))
    .expect("disabled plugin converges");

    assert_eq!(hosts.refreshes(), vec![Host::Claude]);
    assert_eq!(receipt.hosts()[0].status(), ConvergenceStatus::Changed);
    assert!(receipt.hosts()[0].is_enabled());
}

#[test]
fn a_proved_convergence_prunes_superseded_cache_versions_and_says_which() {
    // Twenty releases in, the cache held twenty version directories and 69M,
    // because update only ever added (#451).
    let hosts = FakeHostGateway::with_installations(vec![
        installation(Host::Claude, "0.6.0", "/tmp/claude"),
        installation(Host::Codex, "0.6.0", "/tmp/codex"),
    ]);
    let target = version("0.6.1");
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty();
    let cache = FakePluginCache::holding(&["0.4.2", "0.5.0", "0.5.2", "0.6.0", "0.6.1"]);

    let receipt = UpdateKmp::new(
        &hosts,
        &releases,
        &engines,
        &cache,
        &FakeBridgeStore::default(),
    )
    .execute(request(
        LifecycleAction::Update,
        BTreeSet::from([Host::Claude, Host::Codex]),
        Some(target.clone()),
    ))
    .expect("converged update");

    // 0.6.1 is installed and 0.6.0 is the rollback; the rest is dead weight.
    assert_eq!(cache.removed(), ["0.5.2", "0.5.0", "0.4.2"]);
    assert!(
        !receipt.pruned_caches().is_empty(),
        "a convergence that removed three releases has to say so"
    );
    assert!(
        receipt
            .pruned_caches()
            .iter()
            .all(|(_, pruning)| pruning.kept().is_empty())
    );
}

#[test]
fn a_dry_run_removes_nothing_from_any_cache() {
    let hosts = FakeHostGateway::with_installations(vec![installation(
        Host::Claude,
        "0.6.0",
        "/tmp/claude",
    )]);
    let target = version("0.6.1");
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty();
    let cache = FakePluginCache::holding(&["0.4.2", "0.5.0", "0.6.0"]);

    let receipt = UpdateKmp::new(
        &hosts,
        &releases,
        &engines,
        &cache,
        &FakeBridgeStore::default(),
    )
    .execute(LifecycleRequest::new(
        LifecycleAction::Update,
        BTreeSet::from([Host::Claude]),
        Some(target),
        EngineInstallDir::new("/tmp/shared").expect("shared engine dir"),
        true,
    ))
    .expect("planned update");

    assert!(receipt.is_dry_run());
    assert!(cache.removed().is_empty(), "a plan removes nothing");
    assert!(receipt.pruned_caches().is_empty());
}

// ---------------------------------------------------------------------------
// The lexical-bridge table (#517). Every one of these asserts the same rule
// from a different side: a retrieval aid never decides whether a convergence
// succeeded, and never goes missing without saying so.
// ---------------------------------------------------------------------------

fn bridge_request(choice: BridgeChoice) -> LifecycleRequest {
    request(LifecycleAction::Setup, BTreeSet::new(), None).with_bridge(
        choice,
        Some(BridgeInstallDir::new("/tmp/data/kmp").expect("absolute")),
    )
}

fn current_release() -> (FakeHostGateway, FakeEngineStore, ReleaseVersion) {
    let target = ReleaseVersion::current();
    let hosts = FakeHostGateway::with_installations(vec![
        installation(Host::Claude, target.as_str(), "/tmp/claude"),
        installation(Host::Codex, target.as_str(), "/tmp/codex"),
    ]);
    let engines = FakeEngineStore::running(EngineArtifact::verified(
        target.clone(),
        b"running-engine".to_vec(),
    ));
    (hosts, engines, target)
}

#[test]
fn setup_installs_the_table_the_release_publishes() {
    let (hosts, engines, target) = current_release();
    let releases = FakeReleaseRepository::publishing(target)
        .with_lexical_bridge("table-digest", b"a table".to_vec());
    let tables = FakeBridgeStore::default();

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &tables,
    )
    .execute(bridge_request(BridgeChoice::FromRelease))
    .expect("clean setup");

    assert_eq!(tables.installed_sha256().as_deref(), Some("table-digest"));
    let installed = receipt.lexical_bridge().expect("the receipt says");
    assert!(installed.table_is_present());
    assert!(
        matches!(installed, BridgeInstallation::Installed { path, .. }
            if path == Path::new("/tmp/data/kmp/lexical-bridge.kmpb")),
        "{installed:?}"
    );
}

/// The reason the checksum is a separate call: the table is several megabytes
/// and a second `setup` must not move them again.
#[test]
fn a_machine_that_already_holds_the_published_table_downloads_nothing() {
    let (hosts, engines, target) = current_release();
    let releases = FakeReleaseRepository::publishing(target)
        .with_lexical_bridge("table-digest", b"a table".to_vec());
    let tables = FakeBridgeStore::holding("table-digest");

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &tables,
    )
    .execute(bridge_request(BridgeChoice::FromRelease))
    .expect("clean setup");

    assert_eq!(releases.bridge_downloads(), 0);
    assert!(matches!(
        receipt.lexical_bridge(),
        Some(BridgeInstallation::AlreadyCurrent { .. })
    ));
}

#[test]
fn a_release_that_publishes_no_table_still_converges() {
    let (hosts, engines, target) = current_release();
    let releases = FakeReleaseRepository::publishing(target);
    let tables = FakeBridgeStore::default();

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &tables,
    )
    .execute(bridge_request(BridgeChoice::FromRelease))
    .expect("a table is not a condition of setup");

    assert_eq!(receipt.engine_proofs().len(), 2);
    assert_eq!(tables.installed_sha256(), None);
    let outcome = receipt.lexical_bridge().expect("the receipt says why");
    assert!(!outcome.table_is_present());
    assert!(
        outcome.summary().contains("publishes no table"),
        "{outcome:?}"
    );
}

/// The failure this whole change exists to stop being silent.
#[test]
fn a_filesystem_that_refuses_the_table_does_not_fail_the_convergence() {
    let (hosts, engines, target) = current_release();
    let releases = FakeReleaseRepository::publishing(target)
        .with_lexical_bridge("table-digest", b"a table".to_vec());
    let tables = FakeBridgeStore::refusing("read-only data directory");

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &tables,
    )
    .execute(bridge_request(BridgeChoice::FromRelease))
    .expect("a table that will not install does not undo a proved engine");

    assert_eq!(receipt.engine_proofs().len(), 2);
    let outcome = receipt.lexical_bridge().expect("the receipt says why");
    assert!(!outcome.table_is_present());
    assert!(
        outcome.summary().contains("read-only data directory"),
        "{outcome:?}"
    );
}

#[test]
fn an_operator_who_declines_the_table_is_left_alone() {
    let (hosts, engines, target) = current_release();
    let releases = FakeReleaseRepository::publishing(target)
        .with_lexical_bridge("table-digest", b"a table".to_vec());
    let tables = FakeBridgeStore::default();

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &tables,
    )
    .execute(bridge_request(BridgeChoice::Declined))
    .expect("clean setup");

    assert_eq!(releases.bridge_downloads(), 0);
    assert_eq!(tables.installed_sha256(), None);
    assert_eq!(
        receipt.lexical_bridge(),
        Some(&BridgeInstallation::Declined)
    );
}

#[test]
fn a_table_the_operator_built_is_installed_instead_of_the_published_one() {
    let (hosts, engines, target) = current_release();
    let releases = FakeReleaseRepository::publishing(target)
        .with_lexical_bridge("table-digest", b"a table".to_vec());
    let tables = FakeBridgeStore::default();

    SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &tables,
    )
    .execute(bridge_request(BridgeChoice::FromFile(PathBuf::from(
        "/tmp/es-en.kmpb",
    ))))
    .expect("clean setup");

    assert_eq!(releases.bridge_downloads(), 0);
    assert_eq!(
        tables.installed_sha256().as_deref(),
        Some("operator-digest")
    );
}

/// A plan changes nothing, and that includes the table.
#[test]
fn a_dry_run_installs_no_table_and_claims_none() {
    let (hosts, engines, target) = current_release();
    let releases = FakeReleaseRepository::publishing(target)
        .with_lexical_bridge("table-digest", b"a table".to_vec());
    let tables = FakeBridgeStore::default();
    let planned = LifecycleRequest::new(
        LifecycleAction::Setup,
        BTreeSet::new(),
        None,
        EngineInstallDir::new("/tmp/shared").expect("shared engine dir"),
        true,
    )
    .with_bridge(
        BridgeChoice::FromRelease,
        Some(BridgeInstallDir::new("/tmp/data/kmp").expect("absolute")),
    );

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &tables,
    )
    .execute(planned)
    .expect("plan");

    assert!(receipt.is_dry_run());
    assert_eq!(receipt.lexical_bridge(), None);
    assert_eq!(tables.installed_sha256(), None);
    assert_eq!(releases.bridge_downloads(), 0);
}

#[test]
fn a_platform_with_no_data_home_reports_why_it_has_no_table() {
    let (hosts, engines, target) = current_release();
    let releases = FakeReleaseRepository::publishing(target)
        .with_lexical_bridge("table-digest", b"a table".to_vec());
    let tables = FakeBridgeStore::default();

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &tables,
    )
    .execute(request(LifecycleAction::Setup, BTreeSet::new(), None))
    .expect("clean setup");

    let outcome = receipt.lexical_bridge().expect("the receipt says why");
    assert!(
        outcome.summary().contains("no user data directory"),
        "{outcome:?}"
    );
}

/// The table belongs to the release being converged to, not to the version of
/// the binary doing the converging.
#[test]
fn the_table_comes_from_the_release_the_plan_targets() {
    let target = version("0.9.9");
    let hosts = FakeHostGateway::with_installations(vec![
        installation(Host::Claude, "0.4.2", "/tmp/claude"),
        installation(Host::Codex, "0.4.2", "/tmp/codex"),
    ]);
    let releases = FakeReleaseRepository::publishing(target.clone())
        .with_lexical_bridge("table-digest", b"a table".to_vec());
    let engines = FakeEngineStore::empty();
    let tables = FakeBridgeStore::default();
    let request = request(
        LifecycleAction::Update,
        BTreeSet::from([Host::Claude, Host::Codex]),
        Some(target.clone()),
    )
    .with_bridge(
        BridgeChoice::FromRelease,
        Some(BridgeInstallDir::new("/tmp/data/kmp").expect("absolute")),
    );

    UpdateKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &tables,
    )
    .execute(request)
    .expect("convergence");

    assert_eq!(releases.bridge_asked_for(), vec![target.tag()]);
}
