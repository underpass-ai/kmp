//! Every timestamp argument publishes `"format": "date-time"`, and only
//! timestamp arguments do.
//!
//! A clock argument must be an RFC 3339 timestamp with its zone; a bare date
//! such as `2026-08-01` is refused (`clock::require_rfc3339`). Agents sent
//! bare dates because the published schema did not say so. This test asks
//! the validator itself which arguments are timestamps: every string in an
//! accepted probe call is replaced, one at a time, by a bare date, and the
//! arguments whose refusal is a timestamp refusal are the validator's set.
//! That set must equal the set `tools/list` tags with `date-time`, both ways,
//! so neither a new clock argument nor a stray tag can land unseen.
//!
//! The format is an annotation: the law (`schema_law`) never refuses a value
//! because of it, so the refusal still comes from the reader that parses it.

use std::collections::BTreeSet;

use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const APP_MIME: &str = "text/html;profile=mcp-app";
const BARE_DATE: &str = "2026-08-01";

async fn send(server: &KernelMcpServer, request: Value) -> Value {
    let reply = server
        .handle_json_bytes(request.to_string().as_bytes())
        .await
        .expect("a request is answered");
    serde_json::from_str(&reply).expect("the reply is JSON")
}

async fn call(server: &KernelMcpServer, tool: &str, arguments: &Value) -> Value {
    send(
        server,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
               "params": {"name": tool, "arguments": arguments}}),
    )
    .await["result"]
        .clone()
}

fn ingest(entry: &str, key: &str) -> Value {
    json!({"about": "proj:a", "idempotency_key": key,
        "memory": {"dimensions": [{"id": "task:t1", "kind": "task"}],
            "entries": [{"id": entry, "kind": "observation",
                "text": "Deploy failed on node 7 because the disk was full.",
                "coordinates": [{"dimension": "task", "scope_id": "task:t1"}]}]},
        "provenance": {"source_kind": "agent", "source_agent": "audit",
                       "observed_at": "2026-11-20T10:00:00Z"}})
}

fn clocks() -> Value {
    json!({"occurred_at": "2026-01-01T00:00:00Z", "observed_at": "2026-01-02T00:00:00Z",
           "ingested_at": "2026-01-03T00:00:00Z", "valid_from": "2026-01-01T00:00:00Z",
           "valid_until": "2026-06-01T00:00:00Z"})
}

fn interval() -> Value {
    json!({"start": "2026-01-01T00:00:00Z", "end": "2026-12-01T00:00:00Z"})
}

fn as_of() -> Value {
    json!({"time": "2026-12-01T00:00:00Z"})
}

/// Calls that carry every timestamp argument of every tool, each valid.
fn probes() -> Vec<(&'static str, Value)> {
    let mut coordinate = json!({"dimension": "task", "scope_id": "task:t1"});
    let mut relation_coordinate = coordinate.clone();
    for (key, value) in clocks().as_object().expect("clocks") {
        coordinate[key] = value.clone();
        relation_coordinate[key] = value.clone();
    }
    vec![
        (
            "kmp_ingest",
            json!({"about": "proj:t", "idempotency_key": "probe-ingest",
            "memory": {"dimensions": [{"id": "task:t1", "kind": "task"}],
                "entries": [
                    {"id": "proj:t:a", "kind": "observation", "text": "The disk filled.",
                     "coordinates": [coordinate]},
                    {"id": "proj:t:b", "kind": "observation", "text": "The deploy failed.",
                     "coordinates": [{"dimension": "task", "scope_id": "task:t1"}]}],
                "evidence": [{"id": "evidence:proj:t:ev", "text": "Log line: disk full.",
                    "time": "2026-01-02T00:00:00Z", "supports": ["proj:t:a"],
                    "support_clocks": {"observed_at": "2026-01-02T00:00:00Z",
                                       "ingested_at": "2026-01-03T00:00:00Z"}}],
                "relations": [{"from": "proj:t:b", "to": "proj:t:a", "rel": "depends_on",
                    "class": "causal", "confidence": "high", "why": "A full disk fails a deploy.",
                    "evidence": "Log line: disk full.", "clocks": clocks(),
                    "coordinate": relation_coordinate}]},
            "provenance": {"source_kind": "agent", "source_agent": "audit",
                           "observed_at": "2026-01-02T00:00:00Z"}}),
        ),
        (
            "kmp_write_memory",
            json!({"about": "proj:w", "actor": "audit", "idempotency_key": "probe-write",
            "labels": {"task": ["t1"]}, "source_kind": "agent", "options": {"dry_run": true},
            "observed_at": "2026-01-02T00:00:00Z", "occurred_at": "2026-01-01T00:00:00Z",
            "valid_from": "2026-01-01T00:00:00Z", "valid_until": "2026-06-01T00:00:00Z",
            "memories": [
                {"id": "m1", "kind": "observation", "summary": "Rollback of release 4.2 finished.",
                 "evidence": "Operator log line: rollback ok.",
                 "observed_at": "2026-01-02T00:00:00Z", "occurred_at": "2026-01-01T00:00:00Z",
                 "valid_from": "2026-01-01T00:00:00Z", "valid_until": "2026-06-01T00:00:00Z"},
                {"id": "m2", "kind": "decision", "summary": "Release 4.2 is rolled back.",
                 "evidence": "Change record: rollback approved."}]}),
        ),
        (
            "kmp_write_memory",
            json!({"about": "proj:a", "actor": "audit", "idempotency_key": "probe-link",
            "source_kind": "agent", "options": {"dry_run": true},
            "relations": [{"from": "proj:a:e1", "to": "proj:a:e0", "rel": "depends_on",
                "class": "causal", "why": "The second deploy needs the first.",
                "evidence": "Change record.", "observed_at": "2026-11-20T11:00:00Z"}]}),
        ),
        ("kmp_wake", json!({"about": "proj:a", "as_of": as_of()})),
        (
            "kmp_wake",
            json!({"about": "proj:a", "interval": interval()}),
        ),
        (
            "kmp_ask",
            json!({"about": "proj:a", "question": "Why did the deploy fail?", "as_of": as_of()}),
        ),
        (
            "kmp_ask",
            json!({"about": "proj:a", "question": "Why did the deploy fail?", "interval": interval()}),
        ),
        (
            "kmp_trace",
            json!({"about": "proj:a", "from": "proj:a:e0", "to": "proj:a:e1", "as_of": as_of()}),
        ),
        (
            "kmp_trace",
            json!({"about": "proj:a", "from": "proj:a:e0", "to": "proj:a:e1", "interval": interval()}),
        ),
        (
            "kmp_time",
            json!({"about": "proj:a", "move": "rewind", "from": {"time": "2026-12-01T00:00:00Z"}}),
        ),
        (
            "kmp_time",
            json!({"about": "proj:a", "move": "rewind", "interval": interval()}),
        ),
        (
            "kmp_time",
            json!({"about": "proj:a", "move": "goto", "at": {"time": "2026-12-01T00:00:00Z"}}),
        ),
        (
            "kmp_time",
            json!({"about": "proj:a", "move": "near", "around": {"time": "2026-12-01T00:00:00Z"}}),
        ),
        (
            "kmp_relate",
            json!({"about": "proj:a", "interval": interval()}),
        ),
        (
            "kmp_curate",
            json!({"mode": "review", "about": "proj:a", "interval": interval()}),
        ),
        (
            "kmp_relabel",
            json!({"about": "proj:a", "ref": "proj:a:e0", "actor": "audit",
            "observed_at": "2026-11-20T12:00:00Z", "why": "audit relabel",
            "add": {"incident": ["i1"]},
            "options": {"labels_new": ["incident"], "dry_run": true}}),
        ),
        (
            "kmp_view_apply_intent",
            json!({"idempotency_key": "probe-view", "focus": {"time_range":
                {"from": "2026-01-01T00:00:00Z", "to": "2027-01-01T00:00:00Z"}}}),
        ),
        (
            "kmp_view_read_projection",
            json!({"about": "proj:a", "from": "2026-01-01T00:00:00Z", "to": "2027-01-01T00:00:00Z"}),
        ),
    ]
}

/// `memory.entries[0].valid_from` → `memory.entries[].valid_from`.
fn shape(path: &[String]) -> String {
    let mut out = String::new();
    for step in path {
        if step == "[]" {
            out.push_str("[]");
        } else {
            if !out.is_empty() {
                out.push('.');
            }
            out.push_str(step);
        }
    }
    out
}

fn string_leaves(value: &Value, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match value {
        Value::String(_) => out.push(path.clone()),
        Value::Array(items) => {
            for item in items {
                path.push("[]".into());
                string_leaves(item, path, out);
                path.pop();
            }
        }
        Value::Object(object) => {
            for (key, nested) in object {
                path.push(key.clone());
                string_leaves(nested, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

/// `value` with the string at `path` (first array item at each `[]`, the
/// same one every probe carries its clocks in) set to a bare date.
fn with_bare_date(value: &Value, path: &[String]) -> Value {
    let mut changed = value.clone();
    let mut cursor = &mut changed;
    for step in path {
        cursor = if step == "[]" {
            &mut cursor[0]
        } else {
            &mut cursor[step.as_str()]
        };
    }
    *cursor = json!(BARE_DATE);
    changed
}

fn refusal_text(reply: &Value) -> String {
    let content = &reply["structuredContent"];
    let mut text = content["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    for item in content["feedback"].as_array().into_iter().flatten() {
        text.push(' ');
        text.push_str(item["message"].as_str().unwrap_or_default());
    }
    text
}

fn refuses_as_a_timestamp(reply: &Value) -> bool {
    let text = refusal_text(reply);
    reply["isError"] == true && (text.contains("RFC3339") || text.contains("not a timestamp"))
}

/// Every `(tool, path)` the listed schemas tag `"format": "date-time"`.
fn published(tool: &str, schema: &Value, path: &mut Vec<String>, out: &mut BTreeSet<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    if object.get("format") == Some(&json!("date-time")) {
        out.insert(format!("{tool} {}", shape(path)));
    }
    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        for (key, property) in properties {
            path.push(key.clone());
            published(tool, property, path, out);
            path.pop();
        }
    }
    if let Some(items) = object.get("items") {
        path.push("[]".into());
        published(tool, items, path, out);
        path.pop();
    }
    for keyword in ["additionalProperties", "if", "then", "else", "not"] {
        if let Some(inner) = object.get(keyword) {
            published(tool, inner, path, out);
        }
    }
    for keyword in ["oneOf", "anyOf", "allOf"] {
        for inner in object
            .get(keyword)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            published(tool, inner, path, out);
        }
    }
}

#[tokio::test]
async fn the_published_date_time_arguments_are_exactly_the_ones_the_validator_reads_as_timestamps()
{
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded server");
    send(
        &server,
        json!({"jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {"capabilities": {"extensions": {
            "io.modelcontextprotocol/ui": {"mimeTypes": [APP_MIME]}}}}}),
    )
    .await;
    for (entry, key) in [("proj:a:e0", "seed-0"), ("proj:a:e1", "seed-1")] {
        let reply = call(&server, "kmp_ingest", &ingest(entry, key)).await;
        assert_ne!(reply["isError"], true, "seed: {reply}");
    }
    call(&server, "kmp_view_open", &json!({"about": "proj:a"})).await;
    let listed = send(
        &server,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    let mut tagged = BTreeSet::new();
    for tool in listed["result"]["tools"].as_array().expect("tools") {
        let name = tool["name"].as_str().expect("name");
        published(name, &tool["inputSchema"], &mut Vec::new(), &mut tagged);
    }
    assert!(!tagged.is_empty(), "tools/list tags no timestamp");

    let mut probed = BTreeSet::new();
    let mut read_as_timestamp = BTreeSet::new();
    let mut answers = std::collections::BTreeMap::new();
    let mut fresh = 0_u32;
    for (tool, probe) in probes() {
        let accepted = call(&server, tool, &probe).await;
        assert_ne!(
            accepted["isError"], true,
            "the {tool} probe is accepted as it stands: {accepted}"
        );
        let mut leaves = Vec::new();
        string_leaves(&probe, &mut Vec::new(), &mut leaves);
        for leaf in leaves {
            let name = format!("{tool} {}", shape(&leaf));
            probed.insert(name.clone());
            // A fresh key per call: a reused key with other content is
            // refused as a conflict before its clocks are read.
            let mut changed = with_bare_date(&probe, &leaf);
            if probe.get("idempotency_key").is_some() && leaf != ["idempotency_key"] {
                fresh += 1;
                changed["idempotency_key"] = json!(format!("probe-{fresh}"));
            }
            let reply = call(&server, tool, &changed).await;
            if refuses_as_a_timestamp(&reply) {
                read_as_timestamp.insert(name);
            } else {
                let text = refusal_text(&reply);
                answers.insert(
                    name,
                    if text.is_empty() {
                        "accepted".to_string()
                    } else {
                        text
                    },
                );
            }
        }
    }

    let untagged: Vec<_> = read_as_timestamp.difference(&tagged).collect();
    assert!(
        untagged.is_empty(),
        "the validator reads these as timestamps but tools/list does not tag them \
         `format: date-time`: {untagged:?}"
    );
    let unprobed: Vec<_> = tagged.difference(&probed).collect();
    assert!(
        unprobed.is_empty(),
        "tagged `format: date-time` but no probe sends them; add them to `probes()`: \
         {unprobed:?}"
    );
    let stray: Vec<_> = tagged
        .difference(&read_as_timestamp)
        .map(|name| format!("{name}: {}", answers.get(name).map_or("", String::as_str)))
        .collect();
    assert!(
        stray.is_empty(),
        "tagged `format: date-time` but a bare date there is not refused as a timestamp:\n{}",
        stray.join("\n")
    );
}

/// `format` is an annotation: a call is never refused, or accepted, by the
/// law because of it. A bare date at a tagged argument is refused by the
/// reader that parses it, with the RFC 3339 wording, not as a schema
/// violation of the format.
#[tokio::test]
async fn a_bare_date_is_refused_by_the_reader_not_by_the_format() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded server");
    let reply = call(
        &server,
        "kmp_wake",
        &json!({"about": "proj:a", "as_of": {"time": BARE_DATE}}),
    )
    .await;
    assert!(refuses_as_a_timestamp(&reply), "{reply}");
    assert!(
        !refusal_text(&reply).contains("format"),
        "the refusal is the reader's, not a format check: {reply}"
    );
}
