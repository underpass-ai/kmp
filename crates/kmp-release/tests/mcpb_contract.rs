use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn input_name(triple: &str) -> String {
    let suffix = if triple == "x86_64-pc-windows-msvc" {
        ".exe"
    } else {
        ""
    };
    format!("kmp-mcp-v{}-{triple}{suffix}", env!("CARGO_PKG_VERSION"))
}

#[test]
fn rust_mcpb_packaging_is_complete_deterministic_and_stampable() {
    let scratch = tempfile::tempdir().expect("scratch directory");
    let input = scratch.path().join("input");
    let output = scratch.path().join("output");
    std::fs::create_dir_all(&input).expect("input directory");
    for triple in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ] {
        std::fs::write(input.join(input_name(triple)), format!("binary:{triple}"))
            .expect("fixture binary");
    }

    let package = || {
        Command::new(env!("CARGO_BIN_EXE_kmp-release"))
            .args(["mcpb", "package", env!("CARGO_PKG_VERSION"), "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .arg("--root")
            .arg(repository_root())
            .output()
            .expect("package command runs")
    };
    let first = package();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let archive = output.join(format!("kmp-mcp-v{}.mcpb", env!("CARGO_PKG_VERSION")));
    let first_bytes = std::fs::read(&archive).expect("first MCPB");
    let second = package();
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(first_bytes, std::fs::read(&archive).expect("second MCPB"));

    let reader = std::fs::File::open(&archive).expect("MCPB opens");
    let mut zip = zip::ZipArchive::new(reader).expect("valid zip");
    let mut names = (0..zip.len())
        .map(|index| zip.by_index(index).expect("entry").name().to_string())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names.len(), 11);
    assert!(names.contains(&"manifest.json".to_string()));
    assert!(names.contains(&"server/kmp-mcp.exe".to_string()));
    let mut manifest = String::new();
    zip.by_name("manifest.json")
        .expect("manifest entry")
        .read_to_string(&mut manifest)
        .expect("manifest text");
    assert_eq!(
        serde_json::from_str::<Value>(&manifest).expect("manifest JSON")["version"],
        env!("CARGO_PKG_VERSION")
    );

    let server = scratch.path().join("server.json");
    std::fs::copy(repository_root().join("server.json"), &server).expect("server fixture");
    let stamp = Command::new(env!("CARGO_BIN_EXE_kmp-release"))
        .args(["mcpb", "stamp"])
        .arg(&archive)
        .arg("--server")
        .arg(&server)
        .arg("--root")
        .arg(repository_root())
        .output()
        .expect("stamp command runs");
    assert!(
        stamp.status.success(),
        "{}",
        String::from_utf8_lossy(&stamp.stderr)
    );
    let stamped: Value =
        serde_json::from_str(&std::fs::read_to_string(server).expect("stamped server"))
            .expect("server JSON");
    let package = stamped["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .find(|package| package["registryType"] == "mcpb")
        .expect("MCPB package");
    assert_eq!(package["fileSha256"].as_str().map(str::len), Some(64));
    assert_eq!(
        package["identifier"],
        format!(
            "https://github.com/underpass-ai/kmp/releases/download/v{0}/kmp-mcp-v{0}.mcpb",
            env!("CARGO_PKG_VERSION")
        )
    );
}
