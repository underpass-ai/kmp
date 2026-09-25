use serde_json::Value;
use sha2::{Digest, Sha256};

use super::embedded_backend::EmbeddedKernelMcpBackend;
use super::store_config_report::StoreConfigReport;
use crate::serving::telemetry::captured_log::CapturedLog;

const DEBUG: &str = "kmp_mcp=info,kmp_mcp::store_config=debug";

fn by_file(lines: &[Value], file: &str) -> Value {
    lines
        .iter()
        .find(|line| line["fields"]["file"] == file)
        .cloned()
        .unwrap_or_else(|| panic!("no line for {file}: {lines:?}"))
}

#[test]
fn present_files_are_acknowledged_with_their_hash_and_absent_ones_are_not() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("rerank.json"), r#"{"pool_size":40}"#).expect("rerank");
    std::fs::write(dir.path().join("wake-focus.json"), "{}").expect("focus");
    std::fs::write(dir.path().join("ask-gate.json"), "{}").expect("unknown");
    std::fs::write(dir.path().join("notes.txt"), "not configuration").expect("other");
    let (log, _guard) = CapturedLog::start(DEBUG);

    StoreConfigReport::new(dir.path())
        .beside_store("rerank.json", Ok(()))
        .beside_store("wake-focus.json", Err("needs typesafe.json".into()))
        .beside_store("typesafe.json", Ok(()))
        .at("lexical-bridge.kmpb", None, Ok(()))
        .emit();

    let lines = log.events("kmp_store_config");
    assert_eq!(lines.len(), 3, "{lines:?}");
    let rerank = by_file(&lines, "rerank.json");
    assert_eq!(rerank["target"], "kmp_mcp::store_config");
    assert_eq!(rerank["level"], "DEBUG");
    assert_eq!(rerank["fields"]["status"], "loaded");
    assert_eq!(
        rerank["fields"]["sha256"],
        format!("{:x}", Sha256::digest(br#"{"pool_size":40}"#)).as_str()
    );
    assert_eq!(rerank["fields"]["bytes"], 16);
    let focus = by_file(&lines, "wake-focus.json");
    assert_eq!(focus["level"], "WARN");
    assert_eq!(focus["fields"]["status"], "ignored");
    assert_eq!(focus["fields"]["reason"], "needs typesafe.json");
    let unknown = by_file(&lines, "ask-gate.json");
    assert_eq!(unknown["fields"]["status"], "ignored");
    assert_eq!(
        unknown["fields"]["reason"],
        "not read by this kmp-mcp version"
    );

    let summary = log.events("kmp_store_config_summary");
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0]["fields"]["loaded"], "rerank.json");
    assert_eq!(
        summary[0]["fields"]["ignored"],
        "wake-focus.json,ask-gate.json"
    );
}

#[test]
fn an_ordinary_start_warns_about_ignored_files_and_hashes_nothing_applied() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("rerank.json"), "{}").expect("rerank");
    std::fs::write(dir.path().join("wake-focus.json"), "{}").expect("focus");
    let (log, _guard) = CapturedLog::start("kmp_mcp=info");

    StoreConfigReport::new(dir.path())
        .beside_store("rerank.json", Ok(()))
        .beside_store("wake-focus.json", Err("needs typesafe.json".into()))
        .emit();

    let lines = log.events("kmp_store_config");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["fields"]["file"], "wake-focus.json");
    assert_eq!(lines[0]["level"], "WARN");
    assert!(log.events("kmp_store_config_summary").is_empty());
}

#[test]
fn nothing_is_read_when_the_target_is_off() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("rerank.json"), "{}").expect("rerank");
    let (log, _guard) = CapturedLog::start("kmp_mcp=error");

    StoreConfigReport::new(dir.path())
        .beside_store("rerank.json", Ok(()))
        .emit();

    assert!(log.events("kmp_store_config").is_empty());
    assert!(log.events("kmp_store_config_summary").is_empty());
}

#[test]
fn an_opened_store_says_which_opt_ins_it_could_not_honour() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("rerank.json"), r#"{"pool_size":40}"#).expect("rerank");
    std::fs::write(dir.path().join("write-relations.json"), "{}").expect("write");
    let (log, _guard) = CapturedLog::start(DEBUG);

    let _backend = EmbeddedKernelMcpBackend::open(dir.path()).expect("store");

    let lines = log.events("kmp_store_config");
    let rerank = by_file(&lines, "rerank.json");
    assert_eq!(rerank["fields"]["status"], "ignored");
    assert_eq!(
        rerank["fields"]["reason"],
        "rerank.json needs typesafe.json beside the store"
    );
    let write = by_file(&lines, "write-relations.json");
    assert_eq!(write["fields"]["status"], "ignored");
    // The machine's lexical bridge may or may not be installed; only the
    // opt-ins this test wrote are asserted.
    let summary = &log.events("kmp_store_config_summary")[0];
    let ignored = summary["fields"]["ignored"].as_str().unwrap_or_default();
    assert!(ignored.contains("rerank.json") && ignored.contains("write-relations.json"));
    assert!(!summary["fields"]["loaded"].to_string().contains("rerank"));
}
