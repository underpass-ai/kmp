use std::path::Path;
use std::process::Command;

use kmp_release::adapters::git_cli::GitCli;
use kmp_release::adapters::system_file_system::SystemFileSystem;
use kmp_release::application::use_cases::assemble_candidate::AssembleCandidate;
use kmp_release::application::use_cases::verify_candidate::VerifyCandidate;
use kmp_release::domain::candidate_asset_set::CandidateAssetSet;
use kmp_release::domain::release_version::ReleaseVersion;
use kmp_release::domain::repository_root::RepositoryRoot;
use kmp_release::ports::release_environment::ReleaseEnvironment;
use sha2::{Digest, Sha256};

struct FixtureEnvironment;

impl ReleaseEnvironment for FixtureEnvironment {
    fn value(&self, name: &str) -> Option<String> {
        match name {
            "GITHUB_REF_NAME" => Some("fixture".to_string()),
            "GITHUB_RUN_ID" => Some("42".to_string()),
            _ => None,
        }
    }
}

fn git(root: &Path, arguments: &[&str]) {
    assert!(
        Command::new("git")
            .args(arguments)
            .current_dir(root)
            .status()
            .expect("git")
            .success()
    );
}

fn digest(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn candidate_assembly_and_verification_share_one_exact_asset_contract() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path().join("repository");
    std::fs::create_dir(&root).expect("repository");
    git(&root, &["init", "--initial-branch=main"]);
    git(&root, &["config", "user.name", "KMP contract"]);
    git(
        &root,
        &["config", "user.email", "kmp-contract@example.invalid"],
    );
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace.package]\nversion = \"0.4.2\"\n",
    )
    .expect("Cargo.toml");
    git(&root, &["add", "Cargo.toml"]);
    git(&root, &["commit", "-m", "fixture"]);
    let version = ReleaseVersion::parse("0.4.2").expect("version");
    let source = temporary.path().join("source");
    std::fs::create_dir(&source).expect("source");
    let assets = CandidateAssetSet::for_version(&version);
    let mut mcpb_digest = String::new();
    for name in assets.payloads() {
        let content = format!("fixture:{name}").into_bytes();
        let checksum = digest(&content);
        if name.ends_with(".mcpb") {
            mcpb_digest.clone_from(&checksum);
        }
        std::fs::write(source.join(name), content).expect("asset");
        std::fs::write(
            source.join(format!("{name}.sha256")),
            format!("{checksum}  {name}\n"),
        )
        .expect("checksum");
    }
    std::fs::write(
        root.join("server.json"),
        format!(r#"{{"packages":[{{"registryType":"mcpb","fileSha256":"{mcpb_digest}"}}]}}"#),
    )
    .expect("server.json");
    let repository_root = RepositoryRoot::from_path(&root).expect("root");
    let output = temporary.path().join("candidate");
    let file_system = SystemFileSystem;
    let git = GitCli;
    let environment = FixtureEnvironment;

    let assembled = AssembleCandidate::new(&file_system, &git, &environment)
        .execute(
            &repository_root,
            &version,
            std::slice::from_ref(&source),
            &output,
        )
        .expect("assemble candidate");
    assert_eq!(assembled.run_id, "42");
    let verified = VerifyCandidate::new(&file_system, &git)
        .execute(&repository_root, &version, &output, None, Some("42"))
        .expect("verify candidate");
    assert_eq!(verified, assembled);

    let first = assets.payloads().next().expect("payload");
    std::fs::write(output.join("assets").join(first), b"changed").expect("mutate asset");
    let rejected = VerifyCandidate::new(&file_system, &git)
        .execute(&repository_root, &version, &output, None, Some("42"))
        .expect_err("changed asset must fail");
    assert!(rejected.to_string().contains("checksum does not match"));
}
