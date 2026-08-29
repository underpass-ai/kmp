use std::path::Path;
use std::process::Command;

use kmp_release::domain::public_overview::PublicOverview;

const BEGIN: &str = "<!-- kmp:public-overview:begin -->";
const END: &str = "<!-- kmp:public-overview:end -->";
const OVERVIEW: &str = "KMP gives Codex and Claude Code local-first memory. It stores decisions and evidence on embedded SQLite, not transcripts, through ten memory tools plus three semantic view tools over a shared ChronoLoom view.";

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_kmp-release")
}

fn write_surface(path: &Path, body: &str) {
    std::fs::write(path, format!("header\n{BEGIN}\n{body}\n{END}\ntail\n")).expect("surface");
}

#[test]
fn repository_public_overviews_are_the_same_value() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let canonical = PublicOverview::parse(
        &std::fs::read_to_string(root.join("plugins/kmp/README.md")).expect("plugin README"),
    )
    .expect("canonical overview");
    let repository = PublicOverview::parse(
        &std::fs::read_to_string(root.join("README.md")).expect("repository README"),
    )
    .expect("repository overview");
    let crate_readme = PublicOverview::parse(
        &std::fs::read_to_string(root.join("crates/kmp-mcp/README.md")).expect("crate README"),
    )
    .expect("crate overview");

    assert_eq!(canonical, repository);
    assert_eq!(canonical, crate_readme);
}

#[test]
fn sync_repairs_all_targets_and_is_idempotent() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("source.md");
    let first = directory.path().join("first.md");
    let second = directory.path().join("second.md");
    write_surface(&source, OVERVIEW);
    write_surface(&first, "Stale overview.");
    write_surface(&second, OVERVIEW);
    let common = [
        "--source",
        source.to_str().expect("UTF-8 path"),
        "--target",
        first.to_str().expect("UTF-8 path"),
        "--target",
        second.to_str().expect("UTF-8 path"),
    ];

    assert!(
        Command::new(binary())
            .args(["readme", "sync"])
            .args(common)
            .status()
            .expect("sync")
            .success()
    );
    let synchronized = std::fs::read_to_string(&first).expect("first");
    assert_eq!(
        synchronized,
        std::fs::read_to_string(&second).expect("second")
    );
    assert!(
        Command::new(binary())
            .args(["readme", "sync"])
            .args(common)
            .status()
            .expect("sync replay")
            .success()
    );
    assert_eq!(
        std::fs::read_to_string(first).expect("first replay"),
        synchronized
    );
}

#[test]
fn sync_rejects_a_document_without_the_marker_contract() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("source.md");
    let target = directory.path().join("target.md");
    std::fs::write(&source, "KMP overview without markers\n").expect("source");
    write_surface(&target, OVERVIEW);

    let output = Command::new(binary())
        .args([
            "readme",
            "sync",
            "--source",
            source.to_str().expect("UTF-8 path"),
            "--target",
            target.to_str().expect("UTF-8 path"),
        ])
        .output()
        .expect("sync malformed source");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("marker pair"));
}
