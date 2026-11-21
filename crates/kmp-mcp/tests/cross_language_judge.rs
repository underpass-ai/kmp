//! The cross-language judge of `kmp_ask` (`ask-cross-language.json`), replayed
//! from recorded Jev verdicts: no key, no network.
//!
//! Every case of the shipped-bridge stress fixture
//! (`tests/fixtures/lexical_bridge_stress/cases.json`, synthetic) is asked on a
//! fresh store with the shipped table beside it, `typesafe.json` and the
//! judge's veto and rescue on at their documented lines. Jev answers from
//! `tests/fixtures/cross_language_judge/jev.cassette.json`; a request that was
//! never recorded leaves the judge unavailable, which fails the gate, so a
//! change to what the judge is asked shows up here instead of being guessed.
//!
//! Record again (a real key, after any change to the ranking or the question):
//!
//! ```text
//! KMP_TYPESAFE_CASSETTE_MODE=record KMP_JUDGEMENT_DEADLINES=off \
//!   CROSS_LANGUAGE_GATE_REPORT=1 cargo test -p kmp-mcp --test cross_language_judge -- --nocapture
//! ```
//!
//! with `TYPESAFE_API_KEY` in the environment; move the cassette to a fresh
//! file first so every request is asked once.
use std::path::{Path, PathBuf};

use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const CHILD_ENV: &str = "KMP_CROSS_LANGUAGE_GATE_CHILD";
const NAME: &str = "the_cross_language_judge_holds_its_recorded_verdicts";

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path)
}

fn shipped() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../distribution/lexical-bridge/kmp-lexical-bridge.kmpb")
}

#[test]
fn the_cross_language_judge_holds_its_recorded_verdicts() {
    if std::env::var_os(CHILD_ENV).is_some() {
        tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(gate());
        return;
    }
    let config = tempfile::tempdir().expect("dir");
    let mode = std::env::var("KMP_TYPESAFE_CASSETTE_MODE").unwrap_or_else(|_| "replay".into());
    let mut child = std::process::Command::new(std::env::current_exe().expect("test executable"));
    child
        .args(["--exact", NAME, "--nocapture"])
        .env(CHILD_ENV, "1")
        .env(
            "KMP_TYPESAFE_CASSETTE",
            fixture("cross_language_judge/jev.cassette.json"),
        )
        .env("KMP_TYPESAFE_CASSETTE_MODE", &mode)
        .env("XDG_CONFIG_HOME", config.path())
        .env_remove("KMP_LEXICAL_BRIDGE");
    if mode != "record" {
        child.env_remove("TYPESAFE_API_KEY");
    }
    let status = child.status().expect("child runs");
    assert!(status.success(), "the child check failed");
}

async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let line = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": name, "arguments": arguments}})
    .to_string();
    let response = server.handle_json_line(&line).await.expect("MCP response");
    let value: Value = serde_json::from_str(&response).expect("JSON");
    assert_ne!(value["result"]["isError"], true, "{value}");
    value["result"]["structuredContent"].clone()
}

async fn ask(case: &Value) -> Value {
    ask_with(case, true).await
}

/// One case on a fresh store with the shipped table, `ask-cross-language.json`
/// and, when `typesafe` says so, `typesafe.json`.
async fn ask_with(case: &Value, typesafe: bool) -> Value {
    let dir = tempfile::tempdir().expect("a scratch store");
    #[cfg(unix)]
    std::os::unix::fs::symlink(shipped(), dir.path().join("lexical-bridge.kmpb"))
        .expect("the table beside the store");
    #[cfg(not(unix))]
    std::fs::copy(shipped(), dir.path().join("lexical-bridge.kmpb"))
        .expect("the table beside the store");
    if typesafe {
        std::fs::write(
        dir.path().join("typesafe.json"),
        r#"{"endpoint":"https://api.typesafe.ai/v1/systemone","model":"jev-1.14.0","timeout_ms":20000}"#,
    )
    .expect("typesafe.json");
    }
    std::fs::write(
        dir.path().join("ask-cross-language.json"),
        r#"{"veto":true,"rescue":true}"#,
    )
    .expect("ask-cross-language.json");
    let server = KernelMcpServer::embedded(dir.path()).expect("an embedded store");
    let receipt = call(
        &server,
        "kmp_ingest",
        json!({"about": case["about"], "memory": case["memory"],
               "idempotency_key": format!("xl:{}", case["id"].as_str().unwrap_or_default())}),
    )
    .await;
    assert_eq!(
        receipt["memory"]["read_after_write_ready"], true,
        "{receipt}"
    );
    call(
        &server,
        "kmp_ask",
        json!({"about": case["about"], "question": case["question"],
               "answer_policy": "evidence_or_unknown"}),
    )
    .await
}

/// What the judge did to one answer.
#[derive(Default, Debug)]
struct Tally {
    positives: usize,
    answered_with_gold: usize,
    rescued_with_gold: usize,
    rescued_without_gold: usize,
    rescued_high: usize,
    vetoed: usize,
    negatives: usize,
    negatives_answered: Vec<String>,
    unavailable: Vec<String>,
}

fn marked(answer: &Value, key: &str) -> Vec<String> {
    answer["proof"]["evidence"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["metadata"].get(key).is_some())
        .filter_map(|item| item["source"].as_str().map(String::from))
        .collect()
}

async fn gate() {
    let collection: Value = serde_json::from_str(
        &std::fs::read_to_string(fixture("lexical_bridge_stress/cases.json"))
            .expect("the stress cases are committed"),
    )
    .expect("the stress cases parse");
    let mut tally = Tally::default();
    for case in collection["cases"].as_array().expect("cases") {
        let answer = ask(case).await;
        let id = case["id"].as_str().unwrap_or_default().to_string();
        let warnings = answer["warnings"].to_string();
        if warnings.contains("cross-language judge unavailable") {
            tally.unavailable.push(id.clone());
        }
        let status = answer["answer_status"].as_str().unwrap_or_default();
        let cited = answer["because"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|reason| reason["claim"].as_str().map(String::from))
            .collect::<Vec<_>>();
        let gold = case["judged"]
            .as_array()
            .expect("judged refs")
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect::<Vec<_>>();
        let rescued = marked(&answer, "judged_by")
            .into_iter()
            .any(|source| cited.contains(&source));
        tally.vetoed += usize::from(!marked(&answer, "judged_out").is_empty());
        let answered = status == "answered" || status == "partial";
        if gold.is_empty() {
            tally.negatives += 1;
            if answered {
                tally.negatives_answered.push(id.clone());
            }
        } else {
            tally.positives += 1;
            let with_gold = answered && cited.iter().any(|claim| gold.contains(claim));
            tally.answered_with_gold += usize::from(with_gold);
            if rescued {
                if with_gold {
                    tally.rescued_with_gold += 1;
                } else {
                    tally.rescued_without_gold += 1;
                }
                tally.rescued_high += usize::from(answer["proof"]["confidence"] == "high");
            }
        }
        if rescued {
            assert!(
                answer["summary"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("judgement that it states the answer in another language"),
                "{id}: a rescue says it rests on a judgement: {answer}"
            );
        }
    }
    if std::env::var_os("CROSS_LANGUAGE_GATE_REPORT").is_some() {
        eprintln!("{tally:#?}");
    }
    assert!(
        tally.unavailable.is_empty(),
        "verdicts not recorded (record the cassette again): {:#?}",
        tally.unavailable
    );
    assert!(
        tally.negatives_answered.is_empty(),
        "negatives answered: {:#?}",
        tally.negatives_answered
    );
    assert_eq!(tally.rescued_without_gold, 0, "{tally:#?}");
    assert_eq!(tally.rescued_high, 0, "a rescue is never high: {tally:#?}");
    // Recorded with jev-1.14.0 on 2026-11-21; the floors may rise freely.
    assert!(tally.rescued_with_gold >= RESCUED_FLOOR, "{tally:#?}");
    assert!(tally.answered_with_gold >= ANSWERED_FLOOR, "{tally:#?}");
}

const RESCUED_FLOOR: usize = 6;
const ANSWERED_FLOOR: usize = 33;

/// Without a working `typesafe.json` the file cannot apply: the answer is the
/// deterministic one, with a warning that says why.
#[tokio::test]
async fn without_typesafe_the_judge_is_disabled_and_the_answer_is_deterministic() {
    if std::env::var_os(CHILD_ENV).is_some() {
        return;
    }
    let collection: Value = serde_json::from_str(
        &std::fs::read_to_string(fixture("lexical_bridge_stress/cases.json"))
            .expect("the stress cases are committed"),
    )
    .expect("the stress cases parse");
    let case = collection["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["kind"] == "crosslang")
        .expect("a cross-language case")
        .clone();
    let mut disabled = ask_with(&case, false).await;
    let warnings = disabled["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        warnings.iter().any(|warning| warning
            .as_str()
            .is_some_and(|text| text.starts_with("cross-language judge disabled: ")
                && text.contains("needs typesafe.json"))),
        "{disabled}"
    );
    // The same case on a store with neither file.
    let dir = tempfile::tempdir().expect("a scratch store");
    #[cfg(unix)]
    std::os::unix::fs::symlink(shipped(), dir.path().join("lexical-bridge.kmpb"))
        .expect("the table beside the store");
    #[cfg(not(unix))]
    std::fs::copy(shipped(), dir.path().join("lexical-bridge.kmpb"))
        .expect("the table beside the store");
    let server = KernelMcpServer::embedded(dir.path()).expect("an embedded store");
    call(
        &server,
        "kmp_ingest",
        json!({"about": case["about"], "memory": case["memory"],
               "idempotency_key": format!("xl:{}", case["id"].as_str().unwrap_or_default())}),
    )
    .await;
    let mut plain = call(
        &server,
        "kmp_ask",
        json!({"about": case["about"], "question": case["question"],
               "answer_policy": "evidence_or_unknown"}),
    )
    .await;
    for answer in [&mut disabled, &mut plain] {
        answer["warnings"] = json!([]);
        if let Some(arguments) = answer.pointer_mut("/projection/next_action/arguments") {
            arguments["continuation"] = Value::Null;
        }
        // The warning itself is counted in the page's bytes.
        if let Some(budget) = answer.pointer_mut("/projection/budget") {
            budget["used_bytes"] = Value::Null;
        }
    }
    assert_eq!(disabled, plain);
}
