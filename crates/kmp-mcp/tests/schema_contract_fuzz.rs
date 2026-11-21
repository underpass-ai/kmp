//! The published input schema is the law of every call (E1 of the 1.0
//! review, #921). The law an agent can read is three things: the schema
//! `tools/list` publishes, the closed-objects rule the server instructions
//! state once (an object refuses every key its schema does not list, unless
//! it declares `additionalProperties`), and the cross-field rules each
//! `kmp_guide` card documents under `rules`. For every tool this server
//! lists — the four app tools included — one accepted baseline call is
//! mutated once per keyword the schema declares and once per object for an
//! unknown key, and every documented rule is broken once; every mutation
//! must come back as a boundary `invalid_argument` naming the field (the rule,
//! for a rule), with the server still answering afterwards.
//!
//! The mutations are read from the live `tools/list`, never from a copy, so
//! a schema change is covered the day it lands. Whether a mutation breaks the
//! schema is decided here, independently of the validator under test: only
//! changes that break a keyword on an unconditional path are generated (a
//! type no branch of the node allows, a value outside an `enum`, a number
//! past a bound, an array past its item bounds, a string that does not match
//! its `pattern`, an argument `additionalProperties: false` does not declare,
//! a dropped `required` argument).

use std::collections::BTreeMap;

use kmp_mcp::KernelMcpServer;
use serde_json::{Value, json};

const APP_MIME: &str = "text/html;profile=mcp-app";

fn call(id: u64, name: &str, arguments: &Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": name, "arguments": arguments}})
    .to_string()
}

async fn send(server: &KernelMcpServer, line: String) -> Value {
    let reply = server
        .handle_json_bytes(line.as_bytes())
        .await
        .expect("a request is answered");
    serde_json::from_str(&reply).expect("the reply is JSON")
}

fn ingest(about: &str, entry: &str, key: &str) -> Value {
    json!({"about": about, "idempotency_key": key,
        "memory": {"dimensions": [{"id": "task:t1", "kind": "task"}],
            "entries": [{"id": entry, "kind": "observation",
                "text": "Deploy failed on node 7 because the disk was full.",
                "coordinates": [{"dimension": "task", "scope_id": "task:t1"}]}]},
        "provenance": {"source_kind": "agent", "source_agent": "audit",
                       "observed_at": "2026-11-20T10:00:00Z"}})
}

/// One accepted call per tool; `n` keeps idempotency keys apart.
fn baseline(tool: &str, n: usize) -> Option<Value> {
    Some(match tool {
        "kmp_ingest" => ingest("proj:a", &format!("proj:a:m{n}"), &format!("ingest-{n}")),
        "kmp_write_memory" => json!({"about": "proj:w", "actor": "audit",
            "idempotency_key": format!("write-{n}"), "labels": {"task": ["t1"]},
            "observed_at": "2026-11-20T11:00:00Z", "source_kind": "agent",
            "options": {"strict": true},
            "memories": [{"id": format!("m{n}"), "kind": "observation",
                "summary": format!("Rollback {n} of release 4.2 finished."),
                "evidence": "Operator log line: rollback ok.",
                "valid_from": "2026-01-01T00:00:00Z", "valid_until": "2027-01-01T00:00:00Z"}]}),
        "kmp_wake" => json!({"about": "proj:a", "budget": {"max_bytes": 4000}}),
        "kmp_ask" => json!({"about": "proj:a", "question": "Why did the deploy fail on node 7?",
            "budget": {"max_bytes": 4000}}),
        "kmp_time" => json!({"about": "proj:a", "move": "rewind",
            "from": {"time": "2026-12-01T00:00:00Z"}, "page": {}, "budget": {"max_bytes": 8000}}),
        "kmp_inspect" => json!({"about": "proj:a", "ref": "proj:a:e0", "page": {},
            "budget": {"max_bytes": 4000}}),
        "kmp_relate" => json!({"about": "proj:a"}),
        "kmp_trace" => json!({"about": "proj:a", "from": "proj:a:e0", "to": "proj:a:e1"}),
        "kmp_relabel" => json!({"about": "proj:a", "ref": "proj:a:e0", "actor": "audit",
            "observed_at": "2026-11-20T12:00:00Z", "why": "audit relabel",
            "add": {"incident": ["i1"]},
            "options": {"labels_new": ["incident"], "dry_run": true}}),
        "kmp_condense" => json!({"about": "proj:a", "ref": "proj:a:e0", "actor": "audit",
            "language": "en", "scope": "node_body", "card": "Deploy failed: disk full.",
            "source": {"revision": 1, "record_digest": "sha256:00"},
            "expect": {"absent": true}}),
        "kmp_summaries_audit" => json!({"about": "proj:a"}),
        "kmp_curate" => json!({"mode": "review", "about": "proj:a"}),
        "kmp_guide" => json!({"topic": "write"}),
        "kmp_view_open" => json!({"about": "proj:a"}),
        "kmp_view_get_state" => json!({}),
        "kmp_view_apply_intent" => {
            json!({"idempotency_key": format!("view-{n}"), "search": "disk"})
        }
        "kmp_view_read_projection" => json!({"about": "proj:a",
            "from": "2026-01-01T00:00:00Z", "to": "2027-01-01T00:00:00Z"}),
        "kmp_view_read_nodes" => json!({"about": "proj:a", "refs": ["proj:a:e0"]}),
        "kmp_view_undo" => json!({}),
        "kmp_view_take_control" => json!({"expected_revision": 1}),
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq)]
enum Step {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone)]
struct Mutation {
    label: &'static str,
    path: Vec<Step>,
    /// `None` drops the argument.
    value: Option<Value>,
}

fn kinds_of(schema: &Value) -> Vec<String> {
    match &schema["type"] {
        Value::String(kind) => vec![kind.clone()],
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect(),
        _ => Vec::new(),
    }
}

/// Every JSON type a node can hold: its own `type` and its typed union
/// branches. Empty means the node does not constrain the type.
fn allowed_kinds(schema: &Value) -> Vec<String> {
    let mut kinds = kinds_of(schema);
    for union in ["oneOf", "anyOf"] {
        for branch in schema[union].as_array().into_iter().flatten() {
            kinds.extend(kinds_of(branch));
        }
    }
    if kinds.iter().any(|kind| kind == "number") {
        kinds.push("integer".into());
    }
    kinds
}

fn samples() -> [(&'static str, &'static str, Value); 7] {
    [
        ("string", "type<-string", json!("x")),
        ("integer", "type<-integer", json!(5)),
        ("number", "type<-float", json!(1.5)),
        ("boolean", "type<-boolean", json!(true)),
        ("null", "type<-null", Value::Null),
        ("array", "type<-array", json!(["x"])),
        ("object", "type<-object", json!({"x": 1})),
    ]
}

fn mutations(schema: &Value, value: Option<&Value>, path: Vec<Step>, out: &mut Vec<Mutation>) {
    let push = |out: &mut Vec<Mutation>, label, value: Value| {
        out.push(Mutation {
            label,
            path: path.clone(),
            value: Some(value),
        });
    };
    let allowed = allowed_kinds(schema);
    if !allowed.is_empty() && !path.is_empty() {
        for (kind, label, sample) in samples() {
            if !allowed.iter().any(|allowed| allowed == kind) {
                push(out, label, sample);
            }
        }
    }
    if let Some(Value::Array(choices)) = schema.get("enum")
        && choices.iter().all(Value::is_string)
    {
        push(out, "enum", json!("zz_not_in_enum"));
    }
    if schema.get("const").is_some_and(Value::is_string) {
        push(out, "const", json!("zz_not_const"));
    }
    let integer = kinds_of(schema).iter().any(|kind| kind == "integer");
    if integer && let Some(minimum) = schema["minimum"].as_i64() {
        push(out, "minimum", json!(minimum - 1));
    }
    if integer && let Some(maximum) = schema["maximum"].as_i64() {
        push(out, "maximum", json!(maximum + 1));
    }
    if let Some(pattern) = schema["pattern"].as_str() {
        let bad = "!! not this form !!";
        let compiled = regex::Regex::new(pattern).expect("catalogue patterns compile");
        assert!(!compiled.is_match(bad));
        push(out, "pattern", json!(bad));
    }
    if let Some(maximum) = schema["maxLength"].as_u64() {
        push(out, "maxLength", json!("x".repeat(maximum as usize + 1)));
    }
    if kinds_of(schema).iter().any(|kind| kind == "array") {
        if schema["minItems"]
            .as_u64()
            .is_some_and(|minimum| minimum >= 1)
        {
            push(out, "minItems", json!([]));
        }
        if let Some(maximum) = schema["maxItems"].as_u64() {
            let item = value
                .and_then(|value| value.get(0))
                .cloned()
                .unwrap_or(json!("v"));
            push(
                out,
                "maxItems",
                Value::Array(vec![item; maximum as usize + 1]),
            );
        }
    }
    if let Some(properties) = schema["properties"].as_object() {
        let object = value.and_then(Value::as_object);
        // Closed unless the object says otherwise: the instructions' rule.
        let closed = schema["additionalProperties"] == Value::Bool(false)
            || (schema.get("additionalProperties").is_none()
                && kinds_of(schema).iter().any(|kind| kind == "object"));
        if closed && (object.is_some() || path.is_empty()) {
            let mut unknown = path.clone();
            unknown.push(Step::Key("zz_unknown".into()));
            out.push(Mutation {
                label: "unknown-argument",
                path: unknown,
                value: Some(json!("x")),
            });
        }
        if object.is_some() || path.is_empty() {
            for (key, nested) in properties {
                let mut child = path.clone();
                child.push(Step::Key(key.clone()));
                mutations(
                    nested,
                    object.and_then(|object| object.get(key)),
                    child,
                    out,
                );
            }
            // A continuation tool publishes its first call's `required` under
            // `else` of `if continuation`; the baselines send no continuation.
            let first_call = (schema["if"] == json!({"required": ["continuation"]}))
                .then(|| schema["else"]["required"].as_array())
                .flatten();
            for required in schema["required"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(first_call.into_iter().flatten())
            {
                let Some(key) = required.as_str() else {
                    continue;
                };
                if object.is_some_and(|object| object.contains_key(key)) {
                    let mut child = path.clone();
                    child.push(Step::Key(key.into()));
                    out.push(Mutation {
                        label: "required",
                        path: child,
                        value: None,
                    });
                }
            }
        }
    }
    if let (Some(entries), Some(object)) = (
        schema.get("additionalProperties").filter(|a| a.is_object()),
        value.and_then(Value::as_object),
    ) {
        for (key, nested) in object {
            let mut child = path.clone();
            child.push(Step::Key(key.clone()));
            mutations(entries, Some(nested), child, out);
        }
    }
    if let (Some(items), Some(Value::Array(array))) = (schema.get("items"), value)
        && let Some(first) = array.first()
    {
        let mut child = path.clone();
        child.push(Step::Index(0));
        mutations(items, Some(first), child, out);
    }
    // The one typed union branch that holds this value is walked for its
    // inner keywords; its type was judged above against every branch. When
    // two branches hold the same type, a change could fit the other one, so
    // neither is walked.
    if let Some(value) = value {
        let kind = match value {
            Value::Object(_) => "object",
            Value::Array(_) => "array",
            Value::String(_) => "string",
            _ => "",
        };
        for union in ["oneOf", "anyOf"] {
            let holding = schema[union]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|branch| kinds_of(branch).iter().any(|declared| declared == kind))
                .collect::<Vec<_>>();
            if let [branch] = holding.as_slice() {
                let mut inner = Vec::new();
                mutations(branch, Some(value), path.clone(), &mut inner);
                out.extend(inner.into_iter().filter(|mutation| {
                    !(mutation.path == path && mutation.label.starts_with("type<-"))
                }));
            }
        }
    }
}

fn applied(base: &Value, mutation: &Mutation) -> Value {
    let mut arguments = base.clone();
    let (last, parents) = mutation.path.split_last().expect("a path below the root");
    let mut slot = &mut arguments;
    for step in parents {
        slot = match step {
            Step::Key(key) => &mut slot[key.as_str()],
            Step::Index(index) => &mut slot[*index],
        };
    }
    match (last, &mutation.value) {
        (Step::Key(key), Some(value)) => {
            slot.as_object_mut()
                .expect("parent object")
                .insert(key.clone(), value.clone());
        }
        (Step::Key(key), None) => {
            slot.as_object_mut().expect("parent object").remove(key);
        }
        (Step::Index(index), Some(value)) => slot[*index] = value.clone(),
        (Step::Index(_), None) => unreachable!("items are never dropped"),
    }
    arguments
}

fn dotted(tool: &str, path: &[Step]) -> String {
    let mut text = tool.to_string();
    for step in path {
        match step {
            Step::Key(key) => text.push_str(&format!(".{key}")),
            Step::Index(index) => text.push_str(&format!("[{index}]")),
        }
    }
    text
}

fn refused_by_the_schema(tool: &str, reply: &Value) -> bool {
    let content = &reply["result"]["structuredContent"];
    if content["error"]["code"] != "invalid_argument" {
        return false;
    }
    tool == "kmp_write_memory"
        || content["feedback"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|item| item["code"] == "SCHEMA_VIOLATION" || item["code"] == "UNKNOWN_ARGUMENT")
}

/// Whether the refusal names the mutated field, a parent of it, or (for a
/// conditional rule) the call as a whole.
fn names_the_field(tool: &str, path: &[Step], reply: &Value) -> bool {
    let wanted = dotted(tool, path);
    let field = wanted
        .strip_prefix(tool)
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_string();
    let content = &reply["result"]["structuredContent"];
    let message = content["error"]["message"].as_str().unwrap_or_default();
    let key = path
        .iter()
        .rev()
        .find_map(|step| match step {
            Step::Key(key) => Some(key.clone()),
            Step::Index(_) => None,
        })
        .unwrap_or_default();
    // The writer's planner words its own refusals ("labels must map keys to
    // arrays") and files a top-level field under each memory it applies to.
    let writer_names_it = tool == "kmp_write_memory" && !key.is_empty() && message.contains(&key);
    writer_names_it
        || message.contains(&wanted)
        || (!key.is_empty() && message.contains(&format!("`{key}`")))
        || content["feedback"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|item| {
                item["field"].as_str().is_some_and(|named| {
                    named.is_empty() || field.starts_with(named) || named.starts_with(&field)
                })
            })
}

/// One call per documented rule that breaks only that rule, built from the
/// baselines. `continuation-alone` is broken on every tool whose published
/// schema takes a continuation, with the first argument a handle may not
/// carry beside it.
fn rule_cases(tools: &[Value]) -> Vec<(&'static str, String, Value)> {
    let handle = format!("read_{}", "0".repeat(32));
    let link = json!({"from": "proj:w:a", "to": "proj:w:b", "rel": "supports",
        "why": "w", "evidence": "e"});
    let with = |tool: &str, change: &dyn Fn(&mut Value)| {
        let mut call = baseline(tool, 900).expect("baseline");
        change(&mut call);
        (tool.to_string(), call)
    };
    let mut cases = vec![
        (
            "write.one-form",
            with("kmp_write_memory", &|c| c["relations"] = json!([link])),
        ),
        (
            "write.labels",
            with("kmp_write_memory", &|c| {
                c.as_object_mut().expect("object").remove("labels");
            }),
        ),
        (
            "write.evidence",
            with("kmp_write_memory", &|c| {
                c["memories"][0]
                    .as_object_mut()
                    .expect("memory")
                    .remove("evidence");
            }),
        ),
        (
            "write.link-class",
            with("kmp_write_memory", &|c| {
                c["memories"][0]["connect_to"] = json!([{"ref": "proj:w:entry:other", "rel": "chosen_because",
                "why": "w", "evidence": "e"}]);
            }),
        ),
        (
            "write.relations-form",
            (
                "kmp_write_memory".to_string(),
                json!({"about": "proj:w",
            "actor": "audit", "labels": {"task": ["t1"]}, "relations": [link]}),
            ),
        ),
        (
            "write.summaries-form",
            (
                "kmp_write_memory".to_string(),
                json!({"about": "proj:w",
            "actor": "audit", "labels": {"task": ["t1"]},
            "search_summaries": [{"ref": "proj:w:a", "summary_en": "x"}]}),
            ),
        ),
        (
            "write.summary-content",
            (
                "kmp_write_memory".to_string(),
                json!({"about": "proj:w",
            "actor": "audit", "search_summaries": [{"ref": "proj:w:a"}]}),
            ),
        ),
        (
            "time.cursor",
            with("kmp_time", &|c| {
                c["move"] = json!("goto");
                c.as_object_mut().expect("object").remove("from");
            }),
        ),
        (
            "trace.mode",
            with("kmp_trace", &|c| {
                c["search"] = json!({"seek": ["verified_by"]})
            }),
        ),
        (
            "trace.search-mode",
            with("kmp_trace", &|c| {
                c["search"] = json!({"direction": "incoming", "same_labels": ["x"]});
            }),
        ),
        (
            "trace.bidirectional",
            with("kmp_trace", &|c| {
                c["search"] = json!({"direction": "bidirectional", "paths_per_target": 2});
            }),
        ),
        (
            "ingest.observed",
            with("kmp_ingest", &|c| {
                c["provenance"]
                    .as_object_mut()
                    .expect("object")
                    .remove("observed_at");
            }),
        ),
        (
            "ingest.links-only",
            with("kmp_ingest", &|c| c["memory"]["entries"] = json!([])),
        ),
        (
            "condense.expect",
            with("kmp_condense", &|c| {
                c["expect"] = json!({"absent": true, "card_revision": 1});
            }),
        ),
    ];
    for tool in tools {
        let schema = &tool["inputSchema"];
        if schema["if"] != json!({"required": ["continuation"]}) {
            continue;
        }
        let name = tool["name"].as_str().expect("name");
        // A valid value for an argument the handle cannot carry, so only the
        // rule can refuse the call: the baseline's own, else the first enum
        // value, else an empty object.
        let base = baseline(name, 901).unwrap_or(json!({}));
        let (extra, value) = schema["properties"]
            .as_object()
            .expect("properties")
            .iter()
            .filter(|(key, _)| !["continuation", "about", "actor", "page"].contains(&key.as_str()))
            .find_map(|(key, property)| {
                base.get(key)
                    .cloned()
                    .or_else(|| property["enum"].get(0).cloned())
                    .or_else(|| {
                        kinds_of(property)
                            .contains(&"object".to_string())
                            .then(|| json!({}))
                    })
                    .map(|value| (key.clone(), value))
            })
            .expect("an argument a handle cannot carry");
        let mut call = json!({"continuation": handle});
        call[extra.as_str()] = value;
        cases.push(("continuation-alone", (name.to_string(), call)));
    }
    cases
        .into_iter()
        .map(|(id, (tool, call))| (id, tool, call))
        .collect()
}

/// Every rule id the guide cards document, read live from `kmp_guide`.
async fn documented_rules(server: &KernelMcpServer, guide: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    for (n, topic) in guide["inputSchema"]["properties"]["topic"]["enum"]
        .as_array()
        .expect("topics")
        .iter()
        .enumerate()
    {
        let card = send(
            server,
            call(5000 + n as u64, "kmp_guide", &json!({"topic": topic})),
        )
        .await;
        for id in card["result"]["structuredContent"]["rules"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(id, _)| id.clone())
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

#[tokio::test]
async fn every_call_the_published_schema_refuses_is_refused_at_the_boundary() {
    let dir = tempfile::tempdir().expect("isolated store");
    let server = KernelMcpServer::embedded(dir.path()).expect("embedded server");
    send(
        &server,
        json!({"jsonrpc": "2.0", "id": 0, "method": "initialize",
        "params": {"capabilities": {"extensions": {
            "io.modelcontextprotocol/ui": {"mimeTypes": [APP_MIME]}}}}})
        .to_string(),
    )
    .await;
    for (index, (entry, key)) in [("proj:a:e0", "seed-0"), ("proj:a:e1", "seed-1")]
        .into_iter()
        .enumerate()
    {
        let reply = send(
            &server,
            call(
                index as u64 + 1,
                "kmp_ingest",
                &ingest("proj:a", entry, key),
            ),
        )
        .await;
        assert_ne!(reply["result"]["isError"], true, "seed: {reply}");
    }
    send(
        &server,
        call(3, "kmp_view_open", &json!({"about": "proj:a"})),
    )
    .await;

    let listed = send(
        &server,
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"}).to_string(),
    )
    .await;
    let tools = listed["result"]["tools"].as_array().expect("tools").clone();
    assert!(
        tools.len() >= 20,
        "the negotiated surface lists the app tools too"
    );

    let guide = tools
        .iter()
        .find(|tool| tool["name"] == "kmp_guide")
        .expect("kmp_guide is listed");
    let documented = documented_rules(&server, guide).await;
    let cases = rule_cases(&tools);
    let mut covered = cases
        .iter()
        .map(|(id, _, _)| id.to_string())
        .collect::<Vec<_>>();
    covered.sort();
    covered.dedup();
    let mut expected = documented.clone();
    expected.sort();
    assert_eq!(
        covered, expected,
        "every rule a guide card documents is broken here once, and nothing else"
    );
    let mut rules_accepted = Vec::new();
    for (n, (rule, tool, arguments)) in cases.iter().enumerate() {
        let reply = send(&server, call(6000 + n as u64, tool, arguments)).await;
        let message = reply["result"]["structuredContent"]["error"]["message"]
            .as_str()
            .unwrap_or_default();
        let named = message.contains(&format!("Rule `{rule}`"))
            || reply["result"]["structuredContent"]["feedback"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|item| item["rule"] == *rule);
        if !refused_by_the_schema(tool, &reply) || !named {
            rules_accepted.push(format!(
                "{rule} on {tool} <- {arguments} :: {}",
                reply["result"]["structuredContent"]
                    .to_string()
                    .chars()
                    .take(300)
                    .collect::<String>()
            ));
        }
    }
    assert!(
        rules_accepted.is_empty(),
        "{} documented rules were not refused by name:\n{}",
        rules_accepted.len(),
        rules_accepted.join("\n")
    );

    let mut id = 10;
    let mut counts = BTreeMap::<String, (usize, usize)>::new();
    let mut accepted = Vec::new();
    let mut broken = Vec::new();
    for tool in &tools {
        let name = tool["name"].as_str().expect("name");
        let base = baseline(name, 0).unwrap_or_else(|| panic!("no baseline for `{name}`: add one"));
        id += 1;
        let reply = send(&server, call(id, name, &base)).await;
        if reply.get("error").is_some()
            || refused_by_the_schema(name, &reply)
            || (name == "kmp_write_memory" && reply["result"]["isError"] == true)
        {
            // A refused baseline proves nothing about its mutations.
            broken.push(format!(
                "{name}: {}",
                reply.to_string().chars().take(400).collect::<String>()
            ));
            continue;
        }

        let mut found = Vec::new();
        mutations(&tool["inputSchema"], Some(&base), Vec::new(), &mut found);
        for (n, mutation) in found.iter().enumerate() {
            let base = baseline(name, n + 1).expect("baseline");
            let arguments = applied(&base, mutation);
            id += 1;
            let reply = send(&server, call(id, name, &arguments)).await;
            assert_ne!(reply["error"]["code"], -32603, "{name} panicked: {reply}");
            let entry = counts.entry(name.to_string()).or_default();
            entry.0 += 1;
            if !refused_by_the_schema(name, &reply)
                || !names_the_field(name, &mutation.path, &reply)
            {
                entry.1 += 1;
                accepted.push(format!(
                    "{} {} <- {} :: {}",
                    mutation.label,
                    dotted(name, &mutation.path),
                    mutation
                        .value
                        .as_ref()
                        .map_or("(dropped)".into(), Value::to_string),
                    reply["result"]["structuredContent"]
                        .to_string()
                        .chars()
                        .take(220)
                        .collect::<String>()
                ));
            }
        }
        // Still serving after every mutation of this tool.
        id += 1;
        let alive = send(&server, call(id, "kmp_wake", &json!({"about": "proj:a"}))).await;
        assert_ne!(alive["result"]["isError"], true, "after {name}: {alive}");
    }
    assert!(
        broken.is_empty(),
        "baselines the schema refuses:\n{}",
        broken.join("\n")
    );
    let total: usize = counts.values().map(|(sent, _)| sent).sum();
    eprintln!(
        "schema contract fuzz: {total} mutations over {} tools: {counts:?}",
        counts.len()
    );
    assert!(
        total > 1500,
        "the walk found too few mutations ({total}); the baselines lost coverage"
    );
    assert!(
        accepted.is_empty(),
        "{} of {total} schema-breaking calls were not refused at the boundary:\n{}",
        accepted.len(),
        accepted.join("\n")
    );
}
