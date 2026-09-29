#[path = "lifecycle_support/fake_process_executor.rs"]
mod fake_process_executor;

use fake_process_executor::FakeProcessExecutor;
use kmp_mcp::lifecycle::NativeHostGateway;
use kmp_mcp::lifecycle::adapters::pi_host_adapter::PiHostAdapter;
use kmp_mcp::lifecycle::domain::host::Host;
use kmp_mcp::lifecycle::domain::host_runtime_status::HostRuntimeStatus;
use kmp_mcp::lifecycle::domain::pi_agent_home::PiAgentHome;
use kmp_mcp::lifecycle::domain::release_version::ReleaseVersion;
use kmp_mcp::lifecycle::ports::host_gateway::HostGateway;
use kmp_mcp::lifecycle::ports::process_output::ProcessOutput;

fn command(
    program: &str,
    arguments: &[&str],
    success: bool,
    stdout: &str,
) -> (String, Vec<String>, ProcessOutput) {
    (
        program.to_string(),
        arguments.iter().map(ToString::to_string).collect(),
        ProcessOutput::completed(success, stdout.to_string(), String::new()),
    )
}

#[test]
fn claude_clean_install_uses_the_native_marketplace_and_plugin_invocation() {
    let processes = FakeProcessExecutor::expecting(vec![
        command(
            "claude",
            &["plugin", "marketplace", "update", "underpass"],
            false,
            "",
        ),
        command(
            "claude",
            &[
                "plugin",
                "marketplace",
                "add",
                "underpass-ai/kmp@marketplace",
            ],
            true,
            "",
        ),
        command(
            "claude",
            &[
                "plugin",
                "install",
                "kmp@underpass",
                "--scope",
                "user",
                "--yes",
            ],
            true,
            "",
        ),
        command(
            "claude",
            &["plugin", "list", "--json"],
            true,
            r#"[{"id":"kmp@underpass","version":"0.5.1","enabled":true,"installPath":"/tmp/claude"}]"#,
        ),
    ]);
    let gateway = NativeHostGateway::new(&processes);

    let installed = gateway
        .provision(
            Host::Claude,
            &ReleaseVersion::parse("0.5.1").expect("version"),
        )
        .expect("Claude installation");

    assert_eq!(installed.host(), Host::Claude);
    assert!(processes.is_exhausted());
}

#[test]
fn codex_clean_install_accepts_an_existing_local_marketplace_snapshot() {
    let processes = FakeProcessExecutor::expecting(vec![
        command(
            "codex",
            &["plugin", "marketplace", "upgrade", "underpass", "--json"],
            false,
            "",
        ),
        command(
            "codex",
            &[
                "plugin",
                "marketplace",
                "add",
                "underpass-ai/kmp",
                "--ref",
                "marketplace",
                "--json",
            ],
            false,
            "",
        ),
        command(
            "codex",
            &["plugin", "add", "kmp@underpass", "--json"],
            true,
            r#"{"version":"0.5.1","installedPath":"/tmp/codex"}"#,
        ),
    ]);
    let gateway = NativeHostGateway::new(&processes);

    let installed = gateway
        .provision(
            Host::Codex,
            &ReleaseVersion::parse("0.5.1").expect("version"),
        )
        .expect("Codex installation");

    assert_eq!(installed.host(), Host::Codex);
    assert!(processes.is_exhausted());
}

#[test]
fn hermes_convergence_registers_through_its_own_cli() {
    let config = command(
        "hermes",
        &["config", "get", "mcp_servers"],
        true,
        "other:\n  command: uvx\n  enabled: true\n",
    );
    let registered = command(
        "hermes",
        &["config", "get", "mcp_servers"],
        true,
        "kmp:\n  command: kmp-mcp\n  enabled: true\n",
    );
    let processes = FakeProcessExecutor::expecting(vec![
        config,
        command(
            "sh",
            &["-c", "echo Y | hermes mcp add kmp --command kmp-mcp"],
            true,
            "  ✓ Saved 'kmp' to ~/.hermes/config.yaml (18/18 tools enabled)\n",
        ),
        registered,
    ]);
    let homes = tempfile::tempdir().expect("homes");
    let gateway = NativeHostGateway::with_homes(
        &processes,
        homes.path().join("codex"),
        homes.path().join("hermes"),
    )
    .expect("gateway");

    let installed = gateway
        .provision(Host::Hermes, &ReleaseVersion::current())
        .expect("Hermes installation");

    assert_eq!(installed.host(), Host::Hermes);
    assert!(installed.is_enabled());
    assert!(processes.is_exhausted());
}

/// An updater one release behind the target must still converge Hermes: its
/// installation carries the target it is converged to, not the version of the
/// binary running the update (#868).
#[test]
fn hermes_refresh_answers_the_target_release_not_the_running_updater() {
    let processes = FakeProcessExecutor::expecting(vec![command(
        "hermes",
        &["config", "get", "mcp_servers"],
        true,
        "kmp:\n  command: kmp-mcp\n  enabled: true\n",
    )]);
    let homes = tempfile::tempdir().expect("homes");
    let gateway = NativeHostGateway::with_homes(
        &processes,
        homes.path().join("codex"),
        homes.path().join("hermes"),
    )
    .expect("gateway");
    let current = ReleaseVersion::current().to_string();
    let (major, rest) = current.split_once('.').expect("major");
    let (minor, _) = rest.split_once('.').expect("minor");
    let next = ReleaseVersion::parse(&format!(
        "{major}.{}.0",
        minor.parse::<u64>().expect("minor number") + 1
    ))
    .expect("next release");

    let refreshed = gateway
        .refresh(Host::Hermes, &next)
        .expect("Hermes converges to a newer target");

    assert_eq!(refreshed.version(), &next);
    assert!(processes.is_exhausted());
}

#[test]
fn hermes_runtime_status_reads_the_config_surface() {
    let processes = FakeProcessExecutor::expecting(vec![command(
        "hermes",
        &["config", "get", "mcp_servers"],
        true,
        "kmp:\n  command: kmp-mcp\n  enabled: true\n",
    )]);
    let gateway = NativeHostGateway::new(&processes);

    let status = gateway
        .runtime_status(Host::Hermes)
        .expect("runtime status");

    assert_eq!(status, HostRuntimeStatus::Registered);
    assert!(processes.is_exhausted());
}

/// A Pi agent home under a temporary root, carrying `settings` when given.
fn pi_home(settings: Option<&str>) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("pi root");
    let home = root.path().join(".pi/agent");
    std::fs::create_dir_all(&home).expect("pi home");
    if let Some(settings) = settings {
        std::fs::write(home.join("settings.json"), settings).expect("settings");
    }
    (root, home)
}

/// Pi has no native MCP: the pi-runtime package is the registration, and
/// the convergence mirrors the plugin's skills where Pi scans without running
/// any host command.
#[test]
fn pi_convergence_mirrors_skills_and_reports_registration() {
    let processes = FakeProcessExecutor::expecting(vec![]);
    let (_root, home) = pi_home(Some(r#"{"packages":["../../Documents/ai/pi-runtime"]}"#));
    let homes = tempfile::tempdir().expect("homes");
    let gateway = NativeHostGateway::with_homes(
        &processes,
        homes.path().join("codex"),
        homes.path().join("hermes"),
    )
    .expect("gateway")
    .with_pi_home(&home)
    .expect("pi home");

    let installed = gateway
        .provision(Host::Pi, &ReleaseVersion::current())
        .expect("Pi installation");
    assert_eq!(installed.host(), Host::Pi);
    assert!(installed.is_enabled());
    assert_eq!(installed.root().as_path(), home.as_path());
    assert_eq!(
        gateway.runtime_status(Host::Pi).expect("runtime status"),
        HostRuntimeStatus::Registered
    );
    assert_eq!(
        gateway
            .refresh(Host::Pi, &ReleaseVersion::current())
            .expect("Pi refresh"),
        installed
    );
    assert!(
        gateway
            .runtime_engine(&installed)
            .expect("engine")
            .as_path()
            .ends_with("kmp-mcp")
    );

    let plugin = tempfile::tempdir().expect("plugin root");
    for name in ["kmp-doctor", "kmp-memory"] {
        let skill = plugin.path().join("skills").join(name);
        std::fs::create_dir_all(&skill).expect("plugin skill");
        std::fs::write(skill.join("SKILL.md"), "---\nname: x\n---").expect("skill md");
    }
    let adapter = PiHostAdapter::with_home(&processes, PiAgentHome::new(&home).expect("home"));
    adapter
        .provision(&ReleaseVersion::current(), Some(plugin.path()))
        .expect("mirror and converge");
    assert!(home.join("skills/kmp-doctor/SKILL.md").is_file());
    assert!(home.join("skills/kmp-memory/SKILL.md").is_file());
    assert!(processes.is_exhausted());
}

/// `kmp-mcp setup --pi` does not install Pi packages; without pi-runtime
/// it refuses and says which command does.
#[test]
fn setup_without_underpass_package_warns_to_run_underpass_setup() {
    let processes = FakeProcessExecutor::expecting(vec![]);
    let (_root, home) = pi_home(None);
    let homes = tempfile::tempdir().expect("homes");
    let gateway = NativeHostGateway::with_homes(
        &processes,
        homes.path().join("codex"),
        homes.path().join("hermes"),
    )
    .expect("gateway")
    .with_pi_home(&home)
    .expect("pi home");

    assert_eq!(
        gateway.runtime_status(Host::Pi).expect("runtime status"),
        HostRuntimeStatus::Missing
    );
    let error = gateway
        .provision(Host::Pi, &ReleaseVersion::current())
        .expect_err("Pi without the pi-runtime package is not converged");
    let message = error.to_string();
    assert!(message.contains("underpass setup"), "{message}");
    assert!(message.contains("pi-runtime"), "{message}");
    assert!(processes.is_exhausted());
}
