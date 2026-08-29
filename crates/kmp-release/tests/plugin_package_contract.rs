use std::path::Path;

use flate2::read::GzDecoder;
use kmp_release::adapters::git_cli::GitCli;
use kmp_release::adapters::gzip_tar_plugin_archive_writer::GzipTarPluginArchiveWriter;
use kmp_release::adapters::system_file_system::SystemFileSystem;
use kmp_release::application::use_cases::package_plugin::PackagePlugin;
use kmp_release::domain::plugin_package_kind::PluginPackageKind;
use kmp_release::domain::release_error::ReleaseError;
use kmp_release::domain::release_version::ReleaseVersion;
use kmp_release::domain::repository_root::RepositoryRoot;
use kmp_release::ports::release_binary_version_reader::ReleaseBinaryVersionReader;
use serde_json::Value;

struct WorkspaceBinaryVersion;

impl ReleaseBinaryVersionReader for WorkspaceBinaryVersion {
    fn read_version(&self, _binary: &Path) -> Result<ReleaseVersion, ReleaseError> {
        ReleaseVersion::parse(env!("CARGO_PKG_VERSION"))
    }
}

fn repository_root() -> RepositoryRoot {
    RepositoryRoot::from_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .expect("repository root")
}

fn unpack(archive: &Path, destination: &Path) {
    let file = std::fs::File::open(archive).expect("plugin archive");
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    archive.unpack(destination).expect("plugin archive unpacks");
}

#[test]
fn plugin_package_contains_one_canonical_tree_and_is_deterministic() {
    let scratch = tempfile::tempdir().expect("scratch directory");
    let binary = scratch.path().join("kmp-mcp");
    std::fs::write(&binary, b"reviewed-engine").expect("fixture binary");
    let output = scratch.path().join("output");
    let use_case = PackagePlugin::new(
        &SystemFileSystem,
        &GitCli,
        &WorkspaceBinaryVersion,
        &GzipTarPluginArchiveWriter,
    );
    let first = use_case
        .execute(
            &repository_root(),
            &binary,
            &output,
            PluginPackageKind::Release,
        )
        .expect("first package");
    let first_bytes = std::fs::read(&first.archive).expect("first archive");
    let second = use_case
        .execute(
            &repository_root(),
            &binary,
            &output,
            PluginPackageKind::Release,
        )
        .expect("second package");
    assert_eq!(
        first_bytes,
        std::fs::read(&second.archive).expect("second archive")
    );

    let unpacked = scratch.path().join("unpacked");
    unpack(&second.archive, &unpacked);
    let plugin = unpacked.join("kmp");
    assert!(plugin.join("hooks/hooks.json").is_file());
    assert!(plugin.join("guide/guide.requests.json").is_file());
    assert!(plugin.join("guide/memory.jsonl").is_file());
    assert!(plugin.join("bin/kmp-mcp").is_file());
    assert!(!plugin.join("guide/build-guide.py").exists());
    assert!(!plugin.join("codex").exists());
    for manifest in [".codex-plugin/plugin.json", ".claude-plugin/plugin.json"] {
        let body: Value = serde_json::from_str(
            &std::fs::read_to_string(plugin.join(manifest)).expect("plugin manifest"),
        )
        .expect("valid plugin manifest");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }
    assert_eq!(
        std::fs::read(plugin.join("bin/kmp-mcp")).expect("packaged engine"),
        b"reviewed-engine"
    );
}
