//! `kmp-mcp import --from <store|bundle> --about <about>...` through the real
//! binary, on a hermetic machine (#903): HOME, XDG and the store are all
//! temporary, and the working directory is outside any repository.

#[path = "support/isolated_home.rs"]
mod isolated_home;

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Output;

use kmp_application::{
    MemoryCoordinateData, MemoryData, MemoryDimensionData, MemoryEntryData, MemoryEvidenceData,
    MemoryIngestCommand,
};
use kmp_embedded::EmbeddedKernel;

const ABOUT: &str = "project:carried";

fn write(about: &str, key: &str, text: &str) -> MemoryIngestCommand {
    let timeline = format!("timeline:{about}");
    let entry_id = format!("{about}:decision:{key}");
    MemoryIngestCommand {
        receipt_context: None,
        default_observation_to_ingestion: false,
        neighborhood_review: None,
        about: about.to_string(),
        memory: MemoryData {
            dimensions: vec![MemoryDimensionData {
                id: timeline.clone(),
                kind: "timeline".to_string(),
                title: None,
                metadata: Default::default(),
            }],
            entries: vec![MemoryEntryData {
                id: entry_id.clone(),
                kind: "decision".to_string(),
                text: text.to_string(),
                coordinates: vec![MemoryCoordinateData {
                    dimension: "timeline".to_string(),
                    scope_id: timeline,
                    occurred_at: Some("2026-10-02T09:00:00Z".to_string()),
                    observed_at: None,
                    ingested_at: None,
                    valid_from: None,
                    valid_until: None,
                    sequence: None,
                    rank: None,
                    metadata: Default::default(),
                }],
                metadata: Default::default(),
            }],
            relations: vec![],
            evidence: vec![MemoryEvidenceData {
                support_clocks: None,
                id: format!("evidence:{about}:{key}"),
                supports: vec![entry_id],
                text: format!("Proof for {key}."),
                source: None,
                time: None,
                metadata: Default::default(),
            }],
        },
        provenance: None,
        idempotency_key: format!("{about}:{key}"),
        dry_run: false,
        label_policy: Default::default(),
    }
}

async fn store_with(dir: &Path, writes: &[(&str, &str, &str)]) {
    let kernel = EmbeddedKernel::open(dir).expect("store opens");
    for (about, key, text) in writes {
        kernel
            .service()
            .ingest(write(about, key, text))
            .await
            .expect("ingest");
    }
}

/// A machine with nothing on it but the stores the test names.
struct Machine {
    home: tempfile::TempDir,
    cwd: tempfile::TempDir,
}

impl Machine {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().expect("home"),
            cwd: tempfile::tempdir().expect("cwd"),
        }
    }

    fn run(&self, store: &Path, args: &[&str]) -> Output {
        isolated_home::command(env!("CARGO_BIN_EXE_kmp-mcp"))
            .args(args)
            .current_dir(self.cwd.path())
            .env("HOME", self.home.path())
            .env("XDG_DATA_HOME", self.home.path().join(".local/share"))
            .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
            .env("KMP_MCP_DATA_DIR", store)
            .env("KMP_VIEWER_ADDR", "off")
            .env_remove("KMP_MCP_BACKEND")
            .output()
            .expect("kmp-mcp runs")
    }
}

fn json(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "exit {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.lines().last().expect("a JSON line")).expect("JSON report")
}

fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if !path.to_string_lossy().ends_with("-shm") {
                let name = path
                    .strip_prefix(dir)
                    .expect("test step")
                    .display()
                    .to_string();
                files.insert(name, std::fs::read(&path).expect("file"));
            }
        }
    }
    files
}

#[tokio::test]
async fn import_from_a_store_is_exact_idempotent_and_leaves_the_source_alone() {
    let machine = Machine::new();
    let source = tempfile::tempdir().expect("source");
    store_with(
        source.path(),
        &[
            (ABOUT, "first", "Carry this decision across."),
            ("project:stays", "private", "This about stays behind."),
        ],
    )
    .await;
    let destination = tempfile::tempdir().expect("destination");
    store_with(
        destination.path(),
        &[("project:here", "local", "Local memory.")],
    )
    .await;
    let source_before = files(source.path());
    let from = source.path().to_str().expect("utf8");

    let first = json(&machine.run(
        destination.path(),
        &["import", "--from", from, "--about", ABOUT],
    ));
    assert_eq!(first["events_imported"], 1, "{first}");
    assert_eq!(first["aggregates"][ABOUT], "imported", "{first}");
    assert_eq!(first["abouts"], serde_json::json!([ABOUT]));

    let second = json(&machine.run(
        destination.path(),
        &["import", "--from", from, "--about", ABOUT],
    ));
    assert_eq!(second["events_imported"], 0, "{second}");
    assert_eq!(second["aggregates"][ABOUT], "unchanged", "{second}");

    let document = machine.run(destination.path(), &["document", ABOUT]);
    assert!(document.status.success(), "{document:?}");
    assert!(String::from_utf8_lossy(&document.stdout).contains("Carry this decision across."));
    let stays = machine.run(destination.path(), &["document", "project:stays"]);
    assert!(!stays.status.success(), "only the requested about came in");

    let after = files(source.path());
    for (name, bytes) in &source_before {
        assert_eq!(after.get(name), Some(bytes), "source `{name}` changed");
    }
    for name in after
        .keys()
        .filter(|name| !source_before.contains_key(*name))
    {
        assert!(name.ends_with("-wal") && after[name].is_empty(), "{name}");
    }
}

#[tokio::test]
async fn import_from_a_bundle_refuses_a_different_history_without_writing() {
    let machine = Machine::new();
    let source = tempfile::tempdir().expect("source");
    store_with(source.path(), &[(ABOUT, "first", "The source's decision.")]).await;
    let bundle = machine.cwd.path().join("carried.jsonl");
    let exported = machine.run(
        source.path(),
        &[
            "export",
            bundle.to_str().expect("test step"),
            "--about",
            ABOUT,
        ],
    );
    assert!(exported.status.success(), "{exported:?}");

    let destination = tempfile::tempdir().expect("destination");
    store_with(
        destination.path(),
        &[(ABOUT, "other", "A different decision, same about.")],
    )
    .await;
    let snapshot = machine.cwd.path().join("before.jsonl");
    let before = json(&machine.run(
        destination.path(),
        &["export", snapshot.to_str().expect("test step")],
    ));

    let refused = machine.run(
        destination.path(),
        &[
            "import",
            "--from",
            bundle.to_str().expect("test step"),
            "--about",
            ABOUT,
        ],
    );
    assert_eq!(refused.status.code(), Some(2), "{refused:?}");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("diverge at revision 1"), "{stderr}");
    assert!(stderr.contains(ABOUT), "{stderr}");

    let snapshot = machine.cwd.path().join("after.jsonl");
    let after = json(&machine.run(
        destination.path(),
        &["export", snapshot.to_str().expect("test step")],
    ));
    assert_eq!(after["event_count"], before["event_count"]);
    assert_eq!(after["content_digest"], before["content_digest"]);

    let missing = machine.run(
        destination.path(),
        &[
            "import",
            "--from",
            bundle.to_str().expect("test step"),
            "--about",
            "project:absent",
        ],
    );
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("`project:absent`"));
}

#[test]
fn import_from_and_about_require_each_other_and_prepare_nothing() {
    let machine = Machine::new();
    let store = machine.cwd.path().join("must-not-be-created");
    for args in [
        vec!["import", "--about", ABOUT],
        vec!["import", "--from", "elsewhere"],
        vec![
            "import",
            "bundle.jsonl",
            "--from",
            "elsewhere",
            "--about",
            ABOUT,
        ],
        vec!["import", "--from", "nowhere", "--about", ABOUT],
    ] {
        let output = machine.run(&store, &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
    }
    assert!(
        !store.exists(),
        "refused invocations must not prepare the store"
    );
}
