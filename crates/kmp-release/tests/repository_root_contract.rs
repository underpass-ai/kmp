use std::path::Path;
use std::process::{Command, Output};

fn checkout(root: &Path, version: &str) {
    std::fs::create_dir_all(root.join("crates/kmp-release/src")).expect("fixture");
    std::fs::create_dir_all(root.join("crates/kmp-mcp")).expect("fixture");
    std::fs::create_dir_all(root.join("plugins/kmp")).expect("fixture");
    std::fs::write(
        root.join("Cargo.toml"),
        format!("[workspace]\nmembers = []\n[workspace.package]\nversion = \"{version}\"\n"),
    )
    .expect("fixture");
    std::fs::write(
        root.join("crates/kmp-release/Cargo.toml"),
        format!("[package]\nname = \"kmp-release\"\nversion = \"{version}\"\n"),
    )
    .expect("fixture");
    for (path, body) in [
        ("plugins/kmp/README.md", version),
        ("README.md", "stale"),
        ("crates/kmp-mcp/README.md", "stale"),
    ] {
        std::fs::write(
            root.join(path),
            format!(
                "<!-- kmp:public-overview:begin -->\n{body}\n<!-- kmp:public-overview:end -->\n"
            ),
        )
        .expect("fixture");
    }
}

fn run(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kmp-release"))
        .current_dir(directory)
        .args(arguments)
        .output()
        .expect("run release binary")
}

#[test]
fn one_binary_reads_each_invocation_checkout_from_nested_directories() {
    let directory = tempfile::tempdir().expect("temporary directory");
    for (name, version) in [("first", "0.1.1"), ("second", "0.2.2")] {
        let root = directory.path().join(name);
        checkout(&root, version);
        let output = run(
            &root.join("crates/kmp-release/src"),
            &["version", "current"],
        );
        assert!(output.status.success(), "{:?}", output);
        assert_eq!(
            String::from_utf8(output.stdout)
                .expect("UTF-8 output")
                .trim(),
            version
        );
    }
}

#[test]
fn sync_mutates_only_the_invocation_checkout() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    checkout(&first, "0.1.1");
    checkout(&second, "0.2.2");
    let targets = ["README.md", "crates/kmp-mcp/README.md"];
    let second_before = targets.map(|path| std::fs::read(second.join(path)).expect("fixture"));

    let output = run(&first.join("plugins/kmp"), &["readme", "sync"]);
    assert!(output.status.success(), "{:?}", output);
    let first_source = std::fs::read(first.join("plugins/kmp/README.md")).expect("fixture");
    for (path, before) in targets.iter().zip(&second_before) {
        assert_eq!(
            std::fs::read(first.join(path)).expect("fixture"),
            first_source
        );
        assert_eq!(&std::fs::read(second.join(path)).expect("fixture"), before);
    }

    let output = run(&second, &["readme", "sync"]);
    assert!(output.status.success(), "{:?}", output);
    let second_source = std::fs::read(second.join("plugins/kmp/README.md")).expect("fixture");
    for path in targets {
        assert_eq!(
            std::fs::read(second.join(path)).expect("fixture"),
            second_source
        );
        assert_eq!(
            std::fs::read(first.join(path)).expect("fixture"),
            first_source
        );
    }
}

#[test]
fn outside_a_workspace_requires_explicit_paths() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("checkout");
    checkout(&root, "0.3.3");
    let before = std::fs::read(root.join("README.md")).expect("fixture");
    let output = run(directory.path(), &["readme", "sync"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no KMP workspace"));
    assert_eq!(
        std::fs::read(root.join("README.md")).expect("fixture"),
        before
    );

    let output = run(
        directory.path(),
        &[
            "version",
            "current",
            "--root",
            root.to_str().expect("UTF-8 root"),
        ],
    );
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        String::from_utf8(output.stdout)
            .expect("UTF-8 output")
            .trim(),
        "0.3.3"
    );
}
