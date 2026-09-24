//! #850: `guide assets apply` ingests a development guide into a store the
//! caller names. Without `--data-dir` it used to fall through to the user's
//! selected memory store and replace the installed guide there.

use std::path::PathBuf;

use kmp_release::application::dto::release_command_dto::ReleaseCommandDto;
use kmp_release::application::mappers::release_command_mapper::ReleaseCommandMapper;

fn arguments(extra: &[&str]) -> Vec<String> {
    [
        "guide",
        "assets",
        "apply",
        "--root",
        env!("CARGO_MANIFEST_DIR"),
        "--binary",
        "/bin/kmp-mcp",
    ]
    .iter()
    .chain(extra)
    .map(|value| value.to_string())
    .collect()
}

#[test]
fn apply_refuses_to_pick_a_store_on_its_own() {
    assert!(ReleaseCommandMapper::map(arguments(&[])).is_err());
}

#[test]
fn apply_ingests_into_the_named_store() {
    match ReleaseCommandMapper::map(arguments(&["--data-dir", "/tmp/kmp-guide-dev"])) {
        Ok(ReleaseCommandDto::ApplyGuideAssets { data_dir, .. }) => {
            assert_eq!(data_dir, PathBuf::from("/tmp/kmp-guide-dev"));
        }
        other => panic!("expected ApplyGuideAssets, got {other:?}"),
    }
}
