#[path = "lifecycle_support/fake_process_executor.rs"]
mod fake_process_executor;

use fake_process_executor::FakeProcessExecutor;
use kmp_mcp::lifecycle::NativeHostGateway;
use kmp_mcp::lifecycle::domain::host::Host;
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
    let gateway = NativeHostGateway::new(&processes);

    let installed = gateway
        .provision(
            Host::Hermes,
            &ReleaseVersion::parse("0.18.9").expect("version"),
        )
        .expect("Hermes installation");

    assert_eq!(installed.host(), Host::Hermes);
    assert!(installed.is_enabled());
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

    assert_eq!(
        status,
        kmp_mcp::lifecycle::domain::host_runtime_status::HostRuntimeStatus::Registered
    );
    assert!(processes.is_exhausted());
}
