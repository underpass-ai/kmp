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
use kmp_mcp::lifecycle::FilesystemPluginCache;
use kmp_mcp::lifecycle::PluginCacheRoots;
use kmp_mcp::lifecycle::PruneDeferredCaches;
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
use kmp_mcp::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
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
fn unpinned_update_uses_latest_and_converges_both_hosts_and_the_shared_engine() {
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
    .execute(request(LifecycleAction::Update, selected, None))
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
    // A machine of the two-plugin-host era: Hermes is not on PATH.
    let hosts = FakeHostGateway::with_installations(vec![
        installation(Host::Claude, target.as_str(), "/tmp/claude"),
        installation(Host::Codex, target.as_str(), "/tmp/codex"),
    ])
    .on_path(vec![Host::Claude, Host::Codex]);
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
    // A machine of the two-plugin-host era: Hermes is not on PATH.
    let hosts =
        FakeHostGateway::with_installations(Vec::new()).on_path(vec![Host::Claude, Host::Codex]);
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

// #849: Hermes ships skills and an MCP registration, not the marketplace
// plugin tree. Its installation root is the whole Hermes home, so digesting it
// beside the Claude and Codex trees could never match. Pi is the same shape:
// skills in its agent home, the connection through the underpass-pi package.
fn native_host_machine() -> FakeHostGateway {
    FakeHostGateway::with_installations(vec![
        installation(Host::Claude, "0.4.2", "/tmp/claude"),
        installation(Host::Codex, "0.4.2", "/tmp/codex"),
        installation(Host::Hermes, "0.4.2", "/tmp/hermes"),
        installation(Host::Pi, "0.4.2", "/tmp/pi"),
    ])
}

fn marketplace_digest() -> String {
    format!("sha256:{}", "a".repeat(64))
}

#[test]
fn update_with_hermes_installed_proves_parity_only_between_marketplace_trees() {
    let hosts = native_host_machine();
    let target = version("0.5.2");
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty();

    let receipt = UpdateKmp::new(
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
    .expect("Hermes beside identical Claude and Codex trees must converge");

    assert_eq!(
        hosts.refreshes(),
        vec![Host::Claude, Host::Codex, Host::Hermes, Host::Pi],
        "Hermes and Pi still converge; they only stay out of the tree comparison"
    );
    assert_eq!(
        receipt.plugin_tree().map(ToString::to_string),
        Some(marketplace_digest())
    );
}

#[test]
fn clean_setup_with_hermes_on_path_converges_all_three_hosts() {
    let target = ReleaseVersion::current();
    let hosts = FakeHostGateway::with_installations(Vec::new()).on_path(vec![
        Host::Claude,
        Host::Codex,
        Host::Hermes,
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
    .expect("clean setup with Hermes on PATH");

    assert_eq!(
        hosts.provisions(),
        vec![Host::Claude, Host::Codex, Host::Hermes]
    );
    assert_eq!(receipt.hosts().len(), 3);
    assert_eq!(
        receipt.plugin_tree().map(ToString::to_string),
        Some(marketplace_digest())
    );
}

#[test]
fn a_hermes_only_update_claims_no_marketplace_tree() {
    let hosts = FakeHostGateway::with_installations(vec![installation(
        Host::Hermes,
        "0.4.2",
        "/tmp/hermes",
    )]);
    let target = version("0.5.2");
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty();

    let receipt = UpdateKmp::new(
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
    .expect("Hermes-only update");

    assert_eq!(
        receipt.plugin_tree(),
        None,
        "a Hermes home is not a plugin tree and must not be reported as one"
    );
}

#[test]
fn a_pi_only_update_claims_no_marketplace_tree() {
    let hosts =
        FakeHostGateway::with_installations(vec![installation(Host::Pi, "0.4.2", "/tmp/pi")]);
    let target = version("0.5.2");
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty();

    let receipt = UpdateKmp::new(
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
    .expect("Pi-only update");

    assert_eq!(hosts.refreshes(), vec![Host::Pi]);
    assert_eq!(
        receipt.plugin_tree(),
        None,
        "a Pi agent home is not a plugin tree and must not be reported as one"
    );
    assert_eq!(
        engines.installations(),
        vec![PathBuf::from("/tmp/shared")],
        "Pi consumes the shared engine on PATH, so the update installs it there"
    );
}

/// Every Pi status that is not a usable registration, with the warning an
/// auto-detected setup gives for it.
fn unusable_pi_statuses() -> Vec<(HostRuntimeStatus, &'static str)> {
    vec![
        (
            HostRuntimeStatus::Missing,
            "pi present but underpass-pi not registered; run `underpass setup`",
        ),
        (
            HostRuntimeStatus::Disabled,
            "underpass-pi registered but its KMP extension is excluded",
        ),
        (
            HostRuntimeStatus::Failed("Pi settings.json is not valid JSON: {\"token\"".to_string()),
            "pi settings.json unreadable",
        ),
    ]
}

/// Auto-detection must not break a machine that merely has Pi installed:
/// without a usable underpass-pi package, Pi is skipped with a warning that
/// says what is wrong, and every other host converges.
#[test]
fn autodetected_setup_skips_an_unusable_pi_and_says_why() {
    for (status, expected) in unusable_pi_statuses() {
        let target = ReleaseVersion::current();
        let hosts =
            FakeHostGateway::with_installations(Vec::new()).reporting_for(Host::Pi, status.clone());
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
        .unwrap_or_else(|error| panic!("{status:?} Pi failed an auto-detected setup: {error}"));

        assert_eq!(
            hosts.provisions(),
            vec![Host::Claude, Host::Codex, Host::Hermes],
            "{status:?}"
        );
        let pi = receipt
            .hosts()
            .iter()
            .find(|host| host.host() == Host::Pi)
            .expect("the skipped Pi is still reported");
        assert_eq!(pi.status(), ConvergenceStatus::Skipped, "{status:?}");
        assert!(!pi.is_enabled(), "{status:?}");
        assert_eq!(pi.warning(), Some(expected), "{status:?}");
    }
}

/// A disabled Pi that already holds an installation is skipped too, instead
/// of being refreshed into the same refusal.
#[test]
fn autodetected_setup_skips_an_installed_but_disabled_pi() {
    let target = ReleaseVersion::current();
    let disabled = HostInstallation::discovered(
        Host::Pi,
        target.clone(),
        PluginRoot::new("/tmp/pi").expect("pi home"),
        false,
    );
    let hosts = FakeHostGateway::with_installations(vec![disabled])
        .reporting_for(Host::Pi, HostRuntimeStatus::Disabled);
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
    .expect("a disabled Pi does not fail an auto-detected setup");

    assert!(hosts.refreshes().is_empty());
    let pi = receipt
        .hosts()
        .iter()
        .find(|host| host.host() == Host::Pi)
        .expect("reported");
    assert_eq!(pi.status(), ConvergenceStatus::Skipped);
    assert_eq!(
        pi.root().map(|root| root.as_path().to_path_buf()),
        Some(PathBuf::from("/tmp/pi"))
    );
}

/// A dry run plans the same skip it would perform.
#[test]
fn an_autodetected_dry_run_plans_the_pi_skip() {
    let hosts = FakeHostGateway::with_installations(Vec::new())
        .reporting_for(Host::Pi, HostRuntimeStatus::Missing);
    let target = ReleaseVersion::current();
    let releases = FakeReleaseRepository::publishing(target.clone());
    let engines = FakeEngineStore::empty();
    let dry_run = LifecycleRequest::new(
        LifecycleAction::Setup,
        BTreeSet::new(),
        None,
        EngineInstallDir::new("/tmp/shared").expect("shared engine dir"),
        true,
    );

    let receipt = SetupKmp::new(
        &hosts,
        &releases,
        &engines,
        &FakePluginCache::default(),
        &FakeBridgeStore::default(),
    )
    .execute(dry_run)
    .expect("planned setup");

    let statuses = receipt
        .hosts()
        .iter()
        .map(|host| (host.host(), host.status()))
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        vec![
            (Host::Claude, ConvergenceStatus::PlannedChange),
            (Host::Codex, ConvergenceStatus::PlannedChange),
            (Host::Hermes, ConvergenceStatus::PlannedChange),
            (Host::Pi, ConvergenceStatus::Skipped),
        ]
    );
}

/// Only an explicit `--pi` insists on Pi, so only it fails without a usable
/// package, and it says which command installs it.
#[test]
fn an_explicit_pi_setup_without_a_usable_package_still_fails() {
    for (status, _) in unusable_pi_statuses() {
        let target = ReleaseVersion::current();
        let hosts =
            FakeHostGateway::with_installations(Vec::new()).reporting_for(Host::Pi, status.clone());
        let releases = FakeReleaseRepository::publishing(target.clone());
        let engines = FakeEngineStore::running(EngineArtifact::verified(
            target.clone(),
            b"running-engine".to_vec(),
        ));

        let error = SetupKmp::new(
            &hosts,
            &releases,
            &engines,
            &FakePluginCache::default(),
            &FakeBridgeStore::default(),
        )
        .execute(request(
            LifecycleAction::Setup,
            BTreeSet::from([Host::Pi]),
            None,
        ))
        .expect_err("--pi without a usable underpass-pi package");

        assert!(
            error.to_string().contains("underpass setup"),
            "{status:?}: {error}"
        );
        assert_eq!(hosts.provisions(), vec![Host::Pi], "{status:?}");
    }
}

/// A registered package is converged by auto-detection like any other host.
#[test]
fn autodetection_keeps_a_registered_pi() {
    let target = ReleaseVersion::current();
    let hosts = FakeHostGateway::with_installations(Vec::new())
        .reporting_for(Host::Pi, HostRuntimeStatus::Registered);
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
    .expect("registered Pi converges");

    assert_eq!(
        hosts.provisions(),
        vec![Host::Claude, Host::Codex, Host::Hermes, Host::Pi]
    );
    assert!(receipt.hosts().iter().all(|host| host.warning().is_none()));
}

#[test]
fn a_tree_mismatch_names_the_hosts_whose_trees_differ() {
    let hosts = native_host_machine();
    let target = version("0.5.2");
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
    .expect_err("different Claude and Codex trees must still fail");

    assert!(matches!(error, LifecycleError::TreeMismatch(_)));
    let detail = error.to_string();
    for named in [
        "claude",
        "codex",
        &marketplace_digest(),
        &format!("sha256:{}", "b".repeat(64)),
    ] {
        assert!(detail.contains(named), "`{detail}` does not name `{named}`");
    }
    assert!(
        !detail.contains("hermes"),
        "`{detail}` blames Hermes, which installs no plugin tree"
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
fn a_proved_convergence_names_superseded_cache_versions_and_removes_none_of_them() {
    // Twenty releases in, the cache held twenty version directories and 69M,
    // because update only ever added (#451). Removing them the moment the new
    // release is proved is the opposite mistake: a session open right now is
    // still reading its skills out of one of those directories (#521).
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

    assert!(
        cache.removed().is_empty(),
        "an update deletes no cached release: {:?}",
        cache.removed()
    );
    // 0.6.1 is installed and 0.6.0 is the rollback; the rest is dead weight,
    // named here and collected by the next start.
    for (host, deferral) in receipt.deferred_caches() {
        assert_eq!(
            deferral
                .deferred()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["0.5.2", "0.5.0", "0.4.2"],
            "{host}"
        );
    }
    assert_eq!(receipt.deferred_caches().len(), 2);
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
    assert!(receipt.deferred_caches().is_empty());
}

// ---------------------------------------------------------------------------
// A live session and the cache under it (#521). A Codex session captures its
// skill catalog once, at the start, and every path it captured points inside
// the version directory it started from. These two run against a real cache
// tree, because the whole question is whether a file is still there.
// ---------------------------------------------------------------------------

/// A host plugin cache holding each release, with the skill a session
/// dispatches by path.
fn cache_holding(base: &Path, releases: &[&str]) -> PathBuf {
    let versions = base.join("codex/plugins/cache/underpass/kmp");
    for release in releases {
        let skill = versions.join(release).join("skills/kmp-doctor");
        std::fs::create_dir_all(&skill).expect("skill directory");
        std::fs::write(skill.join("SKILL.md"), b"# kmp-doctor\n").expect("skill");
    }
    versions
}

fn update_to(versions: &Path, installed: &str, target: &ReleaseVersion) {
    let hosts = FakeHostGateway::with_installations(vec![HostInstallation::discovered(
        Host::Codex,
        version(installed),
        PluginRoot::new(versions.join(installed)).expect("plugin root"),
        true,
    )]);
    let releases = FakeReleaseRepository::publishing(target.clone());
    UpdateKmp::new(
        &hosts,
        &releases,
        &FakeEngineStore::empty(),
        &FilesystemPluginCache,
        &FakeBridgeStore::default(),
    )
    .execute(request(
        LifecycleAction::Update,
        BTreeSet::from([Host::Codex]),
        Some(target.clone()),
    ))
    .expect("converged update");
}

#[test]
fn a_session_started_on_an_older_release_can_still_read_its_captured_skill() {
    // The session of #521: it started on 0.11.0 and holds that skill root.
    // While it is open, 0.12.1 is installed. Invoking the captured skill
    // failed with "No such file or directory" because the update had already
    // deleted the directory the session was still pointing at.
    let base = tempfile::tempdir().expect("temp");
    let versions = cache_holding(base.path(), &["0.11.0", "0.12.0", "0.12.1"]);
    let captured = versions.join("0.11.0/skills/kmp-doctor/SKILL.md");
    assert!(
        captured.is_file(),
        "the session's catalog before the update"
    );

    update_to(&versions, "0.11.0", &version("0.12.1"));

    assert!(
        std::fs::read_to_string(&captured).is_ok(),
        "a live session must still dispatch the skill it captured: {}",
        captured.display()
    );
}

#[test]
fn the_next_start_collects_the_release_the_update_left_behind() {
    // The other half of the deferral: without this the cache only grows, and
    // #451 comes back as twenty directories nobody removes.
    let base = tempfile::tempdir().expect("temp");
    let versions = cache_holding(base.path(), &["0.11.0", "0.12.0", "0.12.1"]);
    let target = version("0.12.1");
    update_to(&versions, "0.11.0", &target);

    let collected = PruneDeferredCaches::new(&FilesystemPluginCache).execute(
        &PluginCacheRoots {
            home: base.path().join("home"),
            codex_home: base.path().join("codex"),
        },
        &target,
    );

    assert_eq!(
        collected
            .iter()
            .map(|(host, pruning)| (
                host.to_string(),
                pruning
                    .removed()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        vec![("codex".to_string(), vec!["0.11.0".to_string()])]
    );
    assert!(
        !versions.join("0.11.0").exists(),
        "superseded, and now gone"
    );
    assert!(
        versions.join("0.12.0/skills/kmp-doctor/SKILL.md").is_file(),
        "the rollback stays"
    );
    assert!(
        versions.join("0.12.1/skills/kmp-doctor/SKILL.md").is_file(),
        "the installed release stays"
    );
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

/// The release's table replaces one the machine already held — an operator's
/// own build, an older release's — and the receipt names what it replaced. A
/// table that must outlive `update` belongs beside one store or in
/// `KMP_LEXICAL_BRIDGE`, and the receipt is where an operator learns that.
#[test]
fn replacing_the_table_the_machine_held_is_named_in_the_receipt() {
    let (hosts, engines, target) = current_release();
    let releases = FakeReleaseRepository::publishing(target)
        .with_lexical_bridge("table-digest", b"a table".to_vec());
    let tables = FakeBridgeStore::holding("operator-digest");

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
    assert!(
        matches!(installed, BridgeInstallation::Installed { replaced: Some(previous), .. }
            if previous == "operator-digest"),
        "{installed:?}"
    );
    assert!(
        installed.summary().contains("operator-digest"),
        "{}",
        installed.summary()
    );
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
