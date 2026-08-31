use std::path::Path;
use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_kmp-release")
}

fn git(root: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .expect("git")
}

#[test]
fn claude_uses_a_cloneable_annotated_tag_and_codex_resolves_the_same_tree() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let remote = temporary.path().join("remote.git");
    let root = temporary.path().join("repository");
    assert!(
        Command::new("git")
            .args(["init", "--bare", remote.to_str().expect("remote")])
            .status()
            .expect("bare remote")
            .success()
    );
    std::fs::create_dir(&root).expect("repository");
    assert!(
        git(&root, &["init", "--initial-branch=main"])
            .status
            .success()
    );
    assert!(
        git(&root, &["config", "user.name", "KMP contract"])
            .status
            .success()
    );
    assert!(
        git(
            &root,
            &["config", "user.email", "kmp-contract@example.invalid"]
        )
        .status
        .success()
    );
    let repository_url = format!("file://{}", remote.display());
    std::fs::create_dir_all(root.join(".claude-plugin")).expect("Claude catalog");
    std::fs::create_dir_all(root.join(".agents/plugins")).expect("Codex catalog");
    std::fs::create_dir_all(root.join("plugins/kmp/.claude-plugin")).expect("Claude manifest");
    std::fs::create_dir_all(root.join("plugins/kmp/.codex-plugin")).expect("Codex manifest");
    std::fs::create_dir_all(root.join("plugins/kmp/skills/kmp-memory")).expect("skill");
    let claude_catalog = serde_json::json!({
        "plugins": [{
            "name": "kmp",
            "description": "Local-first memory with a shared ChronoLoom view.",
            "source": {
                "source": "git-subdir",
                "url": repository_url,
                "path": "plugins/kmp",
                "ref": "v0.4.2"
            }
        }]
    });
    let codex_catalog = serde_json::json!({
        "plugins": [{
            "name": "kmp",
            "source": {"source": "local", "path": "./plugins/kmp"}
        }]
    });
    std::fs::write(
        root.join(".claude-plugin/marketplace.json"),
        serde_json::to_vec_pretty(&claude_catalog).expect("Claude JSON"),
    )
    .expect("Claude catalog");
    std::fs::write(
        root.join(".agents/plugins/marketplace.json"),
        serde_json::to_vec_pretty(&codex_catalog).expect("Codex JSON"),
    )
    .expect("Codex catalog");
    for relative in [
        "plugins/kmp/.claude-plugin/plugin.json",
        "plugins/kmp/.codex-plugin/plugin.json",
    ] {
        std::fs::write(root.join(relative), r#"{"version":"0.4.2"}"#).expect("manifest");
    }
    std::fs::write(
        root.join("plugins/kmp/skills/kmp-memory/SKILL.md"),
        "Recover before re-deriving.\n",
    )
    .expect("skill");
    assert!(git(&root, &["add", "."]).status.success());
    assert!(
        git(&root, &["commit", "-m", "fixture release"])
            .status
            .success()
    );
    let expected_commit = String::from_utf8(git(&root, &["rev-parse", "HEAD"]).stdout)
        .expect("commit")
        .trim()
        .to_string();
    assert!(
        git(&root, &["tag", "-a", "v0.4.2", "-m", "Release v0.4.2"])
            .status
            .success()
    );
    assert!(
        git(&root, &["remote", "add", "origin", &repository_url])
            .status
            .success()
    );
    assert!(
        git(&root, &["push", "origin", "main", "refs/tags/v0.4.2"])
            .status
            .success()
    );

    let verified = Command::new(binary())
        .args([
            "marketplace",
            "verify",
            "0.4.2",
            "--root",
            root.to_str().expect("root"),
            "--repository",
            &repository_url,
        ])
        .output()
        .expect("verify marketplace");
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );

    // #448: the engine the plugin installs on every machine that uses KMP
    // lives at `plugins/kmp/bin/kmp-mcp` and is gitignored, so the tag's clone
    // rightly has no `bin/`. Digesting the working directory let that file
    // decide the parity claim, and `marketplace verify` failed on a release
    // that was in fact perfectly consistent.
    std::fs::create_dir_all(root.join("plugins/kmp/bin")).expect("engine directory");
    std::fs::write(root.join("plugins/kmp/bin/kmp-mcp"), b"an installed engine")
        .expect("installed engine");
    std::fs::write(root.join("plugins/kmp/.gitignore"), "/bin/kmp-mcp\n").expect("plugin ignores");
    let with_installed_engine = Command::new(binary())
        .args([
            "marketplace",
            "verify",
            "0.4.2",
            "--root",
            root.to_str().expect("root"),
            "--repository",
            &repository_url,
        ])
        .output()
        .expect("verify marketplace beside an installed engine");
    assert!(
        with_installed_engine.status.success(),
        "an untracked engine is not part of the published plugin: {}",
        String::from_utf8_lossy(&with_installed_engine.stderr)
    );

    // What git does carry still decides. An edit to a tracked file is a real
    // difference between this tree and the tag's, and must still be caught.
    std::fs::write(
        root.join("plugins/kmp/skills/kmp-memory/SKILL.md"),
        "Recover before re-deriving, differently.\n",
    )
    .expect("edited skill");
    let with_edited_skill = Command::new(binary())
        .args([
            "marketplace",
            "verify",
            "0.4.2",
            "--root",
            root.to_str().expect("root"),
            "--repository",
            &repository_url,
        ])
        .output()
        .expect("verify marketplace against an edited tracked file");
    assert!(!with_edited_skill.status.success());
    assert!(
        String::from_utf8_lossy(&with_edited_skill.stderr)
            .contains("do not resolve the exact same plugin tree"),
        "{}",
        String::from_utf8_lossy(&with_edited_skill.stderr)
    );
    std::fs::write(
        root.join("plugins/kmp/skills/kmp-memory/SKILL.md"),
        "Recover before re-deriving.\n",
    )
    .expect("restored skill");

    let impossible = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--branch",
            &expected_commit,
            &repository_url,
            temporary
                .path()
                .join("clone-by-sha")
                .to_str()
                .expect("clone path"),
        ])
        .output()
        .expect("clone by SHA");
    assert!(!impossible.status.success());

    let mut invalid = claude_catalog;
    invalid["plugins"][0]["source"]["ref"] = serde_json::Value::String(expected_commit);
    std::fs::write(
        root.join(".claude-plugin/marketplace.json"),
        serde_json::to_vec_pretty(&invalid).expect("invalid JSON"),
    )
    .expect("invalid catalog");
    let rejected = Command::new(binary())
        .args([
            "marketplace",
            "verify",
            "0.4.2",
            "--root",
            root.to_str().expect("root"),
            "--repository",
            &repository_url,
        ])
        .output()
        .expect("reject bare SHA");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("clonable immutable tag v0.4.2"));
}
