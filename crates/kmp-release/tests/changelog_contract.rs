use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_kmp-release")
}

#[test]
fn prepare_rejects_empty_unreleased_without_changing_the_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let changelog = directory.path().join("CHANGELOG.md");
    let original = "# Changelog\n\n## [Unreleased]\n\n## [0.4.2] - 2026-08-28\n\n### Fixed\n\n- Existing release.\n\n[Unreleased]: https://github.com/underpass-ai/kmp/compare/v0.4.2...HEAD\n[0.4.2]: https://github.com/underpass-ai/kmp/releases/tag/v0.4.2\n";
    std::fs::write(&changelog, original).expect("fixture");

    let output = Command::new(binary())
        .args([
            "changelog",
            "prepare",
            "0.4.3",
            "--path",
            changelog.to_str().expect("UTF-8 path"),
            "--date",
            "2026-08-29",
        ])
        .output()
        .expect("run release binary");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("[Unreleased] is empty"));
    assert_eq!(
        std::fs::read_to_string(changelog).expect("read fixture"),
        original
    );
}

#[test]
fn prepare_is_idempotent_and_check_accepts_the_release() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let changelog = directory.path().join("CHANGELOG.md");
    std::fs::write(
        &changelog,
        "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- A documented change.\n\n## [0.4.2] - 2026-08-28\n\n### Fixed\n\n- Existing release.\n\n[Unreleased]: https://github.com/underpass-ai/kmp/compare/v0.4.2...HEAD\n[0.4.2]: https://github.com/underpass-ai/kmp/releases/tag/v0.4.2\n",
    )
    .expect("fixture");
    let arguments = [
        "changelog",
        "prepare",
        "0.4.3",
        "--path",
        changelog.to_str().expect("UTF-8 path"),
        "--date",
        "2026-08-29",
    ];

    assert!(
        Command::new(binary())
            .args(arguments)
            .status()
            .expect("prepare")
            .success()
    );
    let prepared = std::fs::read_to_string(&changelog).expect("prepared fixture");
    assert!(prepared.contains("## [0.4.3] - 2026-08-29"));
    assert!(
        prepared.contains("[0.4.3]: https://github.com/underpass-ai/kmp/compare/v0.4.2...v0.4.3")
    );
    assert!(
        Command::new(binary())
            .args(arguments)
            .status()
            .expect("replay")
            .success()
    );
    assert_eq!(
        std::fs::read_to_string(&changelog).expect("replayed fixture"),
        prepared
    );
    assert!(
        Command::new(binary())
            .args([
                "changelog",
                "check",
                "0.4.3",
                "--path",
                changelog.to_str().expect("UTF-8 path"),
            ])
            .status()
            .expect("check")
            .success()
    );
}
