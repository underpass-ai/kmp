//! The read half of the summary backfill, exercised where an agent meets it.
//!
//! `kmp_summaries_audit` exists because only the model can write an English
//! search summary and, until now, only a terminal could say which memories
//! needed one. These checks hold it to what that requires: one call answers
//! for a whole about or a named set, the reasons feed `kmp_write_memory`
//! `search_summaries` unchanged, the reading is repeatable and writes
//! nothing, and the doctor's count is the same reading rather than a second
//! one that agrees by luck.

use std::io::Write;
use std::process::{Command, Stdio};

use kmp_mcp::{EmbeddedKernelMcpBackend, KernelMcpServer};
use serde_json::{Value, json};

const ABOUT: &str = "project:relleno";
const OTHER: &str = "project:almacen";

/// Two Spanish memories that owe a rendering, one English memory that needs
/// none, one rendering the lint refuses, and one that stands.
fn seed() -> Value {
    json!({
        "about": ABOUT,
        "idempotency_key": "audit:seed:1",
        "memory": {
            "dimensions": [{"id": "work:main", "kind": "work"}],
            "entries": [
                entry(
                    "decision:valkey",
                    "decision",
                    "Se adoptó Valkey 7.2 para el almacén compartido (ADR-018).",
                    None,
                    1,
                ),
                entry(
                    "observation:english",
                    "observation",
                    "The weekly meeting moved to ten in the morning.",
                    None,
                    2,
                ),
                entry(
                    "observation:auditores",
                    "observation",
                    "Los auditores pidieron el registro completo antes del jueves.",
                    Some("the record"),
                    3,
                ),
                entry(
                    "decision:ventana",
                    "decision",
                    "La ventana de despliegue se movió al martes por la tarde (#469).",
                    Some("The rollout window moved to Tuesday afternoon (#469)."),
                    4,
                ),
            ]
        }
    })
}

fn entry(suffix: &str, kind: &str, text: &str, summary: Option<&str>, sequence: u32) -> Value {
    let mut entry = json!({
        "id": format!("{ABOUT}:{suffix}"),
        "kind": kind,
        "text": text,
        "coordinates": [{
            "dimension": "work",
            "scope_id": "work:main",
            "occurred_at": format!("2026-05-06T{:02}:00:00Z", 9 + sequence),
            "sequence": sequence
        }]
    });
    if let Some(summary) = summary {
        entry["metadata"] = json!({"summary_en": summary, "summary_en_by": "agent:older"});
    }
    entry
}

async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let raw = server
        .handle_json_line(
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments}
            })
            .to_string(),
        )
        .await
        .unwrap_or_else(|| panic!("{name} answers"));
    serde_json::from_str::<Value>(&raw).expect("JSON-RPC")["result"].clone()
}

async fn audit(server: &KernelMcpServer, arguments: Value) -> Value {
    let result = call(server, "kmp_summaries_audit", arguments).await;
    assert_ne!(result["isError"], true, "{result}");
    result["structuredContent"].clone()
}

fn state_of<'a>(body: &'a Value, suffix: &str) -> &'a Value {
    body["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["ref"] == json!(format!("{ABOUT}:{suffix}")))
        .unwrap_or_else(|| panic!("{suffix} is in the reading"))
}

async fn seeded() -> (tempfile::TempDir, KernelMcpServer) {
    let store = tempfile::tempdir().expect("temporary store");
    let backend = EmbeddedKernelMcpBackend::open(store.path()).expect("embedded backend");
    let server = KernelMcpServer::with_embedded_backend(backend);
    let result = call(&server, "kmp_ingest", seed()).await;
    assert_ne!(result["isError"], true, "{result}");
    (store, server)
}

/// One call, one about, every memory with its state and its reasons — the
/// thing an agent could not get without a terminal.
#[tokio::test]
async fn one_call_returns_every_memory_of_one_about_with_its_state_and_reasons() {
    let (_store, server) = seeded().await;

    let body = audit(&server, json!({"about": ABOUT})).await;

    assert_eq!(body["scope"]["selection"], "current_about");
    assert_eq!(body["totals"]["entries"], 4);
    assert_eq!(body["totals"]["owed"], 2);
    assert_eq!(body["totals"]["missing"], 1);
    assert_eq!(body["totals"]["refused"], 1);
    assert_eq!(body["totals"]["not_required"], 1);
    assert_eq!(body["totals"]["stands"], 1);

    // A memory that owes a rendering carries the text to render.
    let owed = state_of(&body, "decision:valkey");
    assert_eq!(owed["state"], "missing");
    assert_eq!(
        owed["text"],
        "Se adoptó Valkey 7.2 para el almacén compartido (ADR-018)."
    );

    // An English memory needs none, and does not pay for a text nobody has
    // to rewrite.
    let english = state_of(&body, "observation:english");
    assert_eq!(english["state"], "not_required");
    assert!(english.get("text").is_none(), "{english}");

    // A refused summary names its faults in the lint's own words, and says
    // who wrote it.
    let refused = state_of(&body, "observation:auditores");
    assert_eq!(refused["state"], "refused");
    assert_eq!(refused["summary_en"], "the record");
    assert_eq!(refused["summary_en_by"], "agent:older");
    assert_eq!(
        refused["faults"],
        json!(["carries 1 informative word, at least 2 are needed"])
    );

    let stands = state_of(&body, "decision:ventana");
    assert_eq!(stands["state"], "stands");
    assert!(stands.get("faults").is_none(), "{stands}");
}

/// The faults the audit returns are the ones the writer already consumes:
/// the repair is `kmp_write_memory` with `search_summaries`, and nothing is
/// translated on the way.
#[tokio::test]
async fn the_reasons_feed_the_writer_unchanged_and_the_repair_moves_the_state() {
    let (_store, server) = seeded().await;
    let before = audit(&server, json!({"about": ABOUT})).await;
    let refused = state_of(&before, "observation:auditores").clone();

    // The refusal the writer answers with is the same sentence the audit
    // reported, so an agent that reads one has read the other.
    let rejected = call(
        &server,
        "kmp_write_memory",
        json!({
            "about": ABOUT,
            "actor": "agent:backfill",
            "search_summaries": [{
                "ref": refused["ref"],
                "summary_en": "the record"
            }]
        }),
    )
    .await;
    assert_eq!(rejected["isError"], true, "{rejected}");
    assert!(
        rejected.to_string().contains("carries 1 informative word"),
        "the writer refuses in the audit's words: {rejected}"
    );

    let accepted = call(
        &server,
        "kmp_write_memory",
        json!({
            "about": ABOUT,
            "actor": "agent:backfill",
            "search_summaries": [{
                "ref": refused["ref"],
                "summary_en": "The auditors asked for the complete record before Thursday."
            }]
        }),
    )
    .await;
    assert_eq!(
        accepted["structuredContent"]["accepted"], true,
        "{accepted}"
    );

    let after = audit(&server, json!({"about": ABOUT})).await;
    let repaired = state_of(&after, "observation:auditores");
    assert_eq!(repaired["state"], "stands");
    assert_eq!(repaired["summary_en_by"], "agent:backfill");
    assert_eq!(after["totals"]["owed"], 1);
    assert_eq!(
        after["entries"].as_array().expect("entries").len(),
        4,
        "the repair replaces a rendering; it never removes a memory"
    );
}

/// A weakness is a warning: the memory is intact, the reading still returns
/// it, and nothing about it is refused.
#[tokio::test]
async fn a_weak_summary_is_a_warning_and_never_a_refusal() {
    let (_store, server) = seeded().await;

    // The same text, rewritten, with the old rendering left in place: the
    // summary now describes a text the store no longer holds.
    let rewritten = call(
        &server,
        "kmp_ingest",
        json!({
            "about": ABOUT,
            "idempotency_key": "audit:seed:2",
            "memory": {
                "dimensions": [],
                "entries": [{
                    "id": format!("{ABOUT}:decision:ventana"),
                    "kind": "decision",
                    "text": "La ventana de despliegue volvió al jueves por la noche (#469).",
                    "metadata": {
                        "summary_en": "The rollout window moved to Tuesday afternoon (#469).",
                        "summary_en_by": "agent:older"
                    },
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:main",
                        "occurred_at": "2026-05-06T13:00:00Z",
                        "sequence": 4
                    }]
                }]
            }
        }),
    )
    .await;
    assert_ne!(rewritten["isError"], true, "{rewritten}");

    let body = audit(&server, json!({"about": ABOUT})).await;
    let stale = state_of(&body, "decision:ventana");

    assert_eq!(stale["state"], "stands", "a weakness never refuses");
    assert_eq!(stale["weaknesses"][0]["signal"], "stale");
    assert!(
        stale["weaknesses"][0]["says"]
            .as_str()
            .expect("a weakness says what to change")
            .contains("render the text as it now stands")
    );
    assert_eq!(
        stale["text"], "La ventana de despliegue volvió al jueves por la noche (#469).",
        "the rewrite carries the text the new rendering is written from"
    );
    assert_eq!(body["totals"]["weak"], 1);
    assert_eq!(
        body["totals"]["owed"], 2,
        "a weak summary is never counted as a debt"
    );
}

/// Deterministic and repeatable, and a read: two calls answer identically
/// and the store is where it was.
#[tokio::test]
async fn the_reading_repeats_and_leaves_the_store_exactly_as_it_was() {
    let (_store, server) = seeded().await;
    let question = json!({
        "about": ABOUT,
        "question": "Which store engine was adopted (ADR-018)?"
    });

    let before = call(&server, "kmp_ask", question.clone()).await;
    let first = audit(&server, json!({"about": ABOUT})).await;
    let second = audit(&server, json!({"about": ABOUT})).await;
    let after = call(&server, "kmp_ask", question).await;

    assert_eq!(first, second, "the same store reads the same way twice");
    assert_eq!(
        before["structuredContent"]["answer"], after["structuredContent"]["answer"],
        "the audit is a read of quality, not a change to ranking"
    );
    assert_eq!(
        before["structuredContent"]["because"],
        after["structuredContent"]["because"]
    );
}

/// The middle case the terminal never had: several abouts in one call, in
/// the words the reads already use.
#[tokio::test]
async fn a_named_set_reads_several_abouts_and_every_anchor_is_the_opt_in() {
    let (_store, server) = seeded().await;
    let other = call(
        &server,
        "kmp_ingest",
        json!({
            "about": OTHER,
            "idempotency_key": "audit:other:1",
            "memory": {
                "dimensions": [{"id": "work:main", "kind": "work"}],
                "entries": [{
                    "id": format!("{OTHER}:observation:menu"),
                    "kind": "observation",
                    "text": "El menú del comedor se publicó en el tablón del pasillo.",
                    "coordinates": [{
                        "dimension": "work",
                        "scope_id": "work:main",
                        "occurred_at": "2026-05-07T09:00:00Z",
                        "sequence": 1
                    }]
                }]
            }
        }),
    )
    .await;
    assert_ne!(other["isError"], true, "{other}");

    let one = audit(&server, json!({"about": ABOUT})).await;
    assert_eq!(one["totals"]["entries"], 4);

    let set = audit(
        &server,
        json!({
            "about": ABOUT,
            "dimensions": {"scope": "abouts", "abouts": [OTHER]}
        }),
    )
    .await;
    assert_eq!(set["scope"]["selection"], "abouts");
    assert_eq!(set["totals"]["entries"], 5);
    assert_eq!(set["totals"]["owed"], 3);
    assert_eq!(set["abouts"].as_array().expect("per about").len(), 2);

    let every = audit(&server, json!({"dimensions": {"scope": "all_abouts"}})).await;
    assert_eq!(every["scope"]["selection"], "all_abouts");
    assert_eq!(every["totals"]["entries"], 5);

    // The default is one about, and it says how to widen rather than
    // widening itself.
    let refused = call(&server, "kmp_summaries_audit", json!({})).await;
    assert_eq!(refused["isError"], true, "{refused}");
    assert!(refused.to_string().contains("all_abouts"), "{refused}");
}

/// A large store cannot answer in one packet, and a continuation is the
/// existing convention: bound arguments unchanged, a cursor, a complete
/// next action.
#[tokio::test]
async fn the_pages_reconstruct_the_whole_reading_and_a_moved_selection_is_refused() {
    let (_store, server) = seeded().await;

    let mut arguments = json!({"about": ABOUT, "page": {"entries": 2}});
    let mut read = Vec::new();
    let mut first_cursor = None;
    loop {
        let body = audit(&server, arguments.clone()).await;
        assert_eq!(body["totals"]["entries"], 4, "totals never page");
        for entry in body["entries"].as_array().expect("entries") {
            read.push(entry["ref"].as_str().expect("ref").to_string());
        }
        first_cursor =
            first_cursor.or_else(|| body["page"]["next_cursor"].as_str().map(str::to_string));
        match body["next_actions"].as_array().expect("actions").first() {
            None => {
                assert_eq!(body["page"]["has_more"], false);
                break;
            }
            Some(action) => {
                assert_eq!(action["tool"], "kmp_summaries_audit");
                arguments = action["arguments"].clone();
                assert_eq!(arguments["about"], ABOUT, "bound arguments are preserved");
            }
        }
    }
    assert_eq!(read.len(), 4);
    assert_eq!(read[0], format!("{ABOUT}:decision:valkey"));

    // A cursor outlives its reading: repairing a summary changes what the
    // page would hold, so the continuation is refused rather than paging a
    // different reading as if it were the same one.
    let repaired = call(
        &server,
        "kmp_write_memory",
        json!({
            "about": ABOUT,
            "actor": "agent:backfill",
            "search_summaries": [{
                "ref": format!("{ABOUT}:observation:auditores"),
                "summary_en": "The auditors asked for the complete record before Thursday."
            }]
        }),
    )
    .await;
    assert_eq!(
        repaired["structuredContent"]["accepted"], true,
        "{repaired}"
    );

    let stale = call(
        &server,
        "kmp_summaries_audit",
        json!({
            "about": ABOUT,
            "page": {"entries": 2, "cursor": first_cursor.expect("a first continuation")}
        }),
    )
    .await;
    assert_eq!(stale["isError"], true, "{stale}");
    assert!(
        stale.to_string().contains("read the audit again"),
        "{stale}"
    );
}

/// The acceptance criterion that cannot be checked inside one process: what
/// the doctor prints and what the tool returns are one reading, and the
/// terminal's list is the debt half of it.
#[test]
fn the_doctor_the_terminal_and_the_tool_report_one_reading() {
    let store = tempfile::tempdir().expect("temporary store");
    let envs = [
        ("KMP_MCP_BACKEND", "embedded"),
        ("KMP_MCP_DATA_DIR", store.path().to_str().expect("utf8")),
        ("KMP_VIEWER_ADDR", "off"),
    ];
    let ingest = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "kmp_ingest", "arguments": seed()}
    });
    let seeded = run(&envs, &[], &format!("{ingest}\n"));
    assert!(seeded.status.success(), "{seeded:?}");

    let request = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "kmp_summaries_audit", "arguments": {"about": ABOUT}}
    });
    let answered = run(&envs, &[], &format!("{request}\n"));
    let body: Value = serde_json::from_slice(&answered.stdout).expect("a JSON-RPC line");
    let totals = &body["result"]["structuredContent"]["totals"];
    assert_eq!(totals["owed"], 2, "{body}");

    let doctor = run(&envs, &["doctor"], "");
    let report = String::from_utf8_lossy(&doctor.stdout);
    assert!(
        report.contains("search summaries: 2 memories owe one, across 1 about"),
        "the doctor counts what the audit returns: {report}"
    );

    let pending = run(&envs, &["summaries", "pending", "--json"], "");
    let pending: Value = serde_json::from_slice(&pending.stdout).expect("pending is JSON");
    assert_eq!(
        pending.as_array().expect("a list").len(),
        totals["owed"].as_u64().expect("a count") as usize,
        "the terminal lists exactly the debt the audit counted"
    );

    // The refusal that stops a second about is gone: the reading takes a set.
    let both = run(&envs, &["summaries", "pending", ABOUT, OTHER], "");
    assert!(both.status.success(), "{both:?}");
    assert!(
        String::from_utf8_lossy(&both.stdout).contains("2 memories owe"),
        "{both:?}"
    );
}

fn run(envs: &[(&str, &str)], args: &[&str], stdin: &str) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kmp-mcp"));
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.env_remove("KMP_KERNEL_GRPC_ENDPOINT");
    for (name, value) in envs {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("the engine starts");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(stdin.as_bytes())
        .expect("stdin is written");
    drop(child.stdin.take());
    child.wait_with_output().expect("the engine exits")
}
