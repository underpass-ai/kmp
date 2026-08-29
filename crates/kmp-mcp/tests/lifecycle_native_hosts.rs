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
