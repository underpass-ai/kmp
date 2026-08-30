//! The refactor in #404 moves every file under `src/**` without changing what
//! the server advertises or what it answers. That promise is too big to review
//! by reading diffs, so both halves are pinned against checked-in files.
//!
//! - `fixtures/contract/tools_list.json` — what the server advertises.
//! - `fixtures/contract/responses/<tool>.json` — what each tool answers, by
//!   calling it over JSON-RPC.
//!
//! The calls run against the **embedded** backend on a temporary SQLite store,
//! not the fixture backend. That matters: the fixture backend replays canned
//! kernel JSON verbatim, so it never reaches the proto-to-JSON mappers or the
//! byte budgets in `kmp.rs` — the very code this refactor splits. Writing to a
//! real store first and reading it back drags argument validation, the write
//! planner, the mappers, the budgets and the view state through the same run.
//!
//! When a change is genuinely intended, regenerate with
//! `KMP_BLESS_TOOL_SURFACE=1 cargo test -p kmp-mcp --test tool_surface_parity`
//! and review the diff as the contract change it is.

use std::path::{Path, PathBuf};

use kmp_mcp::{EmbeddedKernelMcpBackend, KernelMcpServer};
use serde_json::{Value, json};

const BLESS: &str = "KMP_BLESS_TOOL_SURFACE";
const ABOUT: &str = "question:parity";
const APP_MIME: &str = "text/html;profile=mcp-app";
const CLAIM: &str = "question:parity:claim:denver";
const DETAIL: &str = "question:parity:claim:denver-detail";
const CURRENT: &str = "question:parity:claim:boulder-current";

/// One call per advertised tool, in an order that leaves the store able to
/// answer the reads: the writes come first, then the recalls, then the views.
///
/// `kmp_ask` is called twice — once plainly and once under a budget tight
/// enough to force the recall projection to trim — because trimming is where a
/// refactor is most likely to go quiet.
fn calls() -> Vec<(&'static str, Value)> {
    vec![
        (
            "kmp_ingest",
            json!({
                "about": ABOUT,
                "idempotency_key": "parity:ingest:1",
                "provenance": {
                    "source_kind": "agent",
                    "source_agent": "parity-test",
                    "observed_at": "2026-04-12T15:00:00Z"
                },
                "memory": {
                    "dimensions": [{"id": "conversation:rachel", "kind": "conversation"}],
                    "entries": [
                        {
                            "id": CLAIM,
                            "kind": "claim",
                            "text": "Rachel said she was moving to Denver.",
                            "coordinates": [{
                                "dimension": "conversation",
                                "scope_id": "conversation:rachel",
                                "occurred_at": "2026-04-12T15:00:00Z",
                                "sequence": 1
                            }]
                        },
                        {
                            "id": DETAIL,
                            "kind": "claim",
                            "text": "She starts the new job in Denver in June.",
                            // The optional half of the coordinate mapper —
                            // valid_from, valid_until and rank — is emitted
                            // for every coordinate in every temporal, inspect
                            // and projection answer, so leave none of it
                            // unwritten here.
                            "coordinates": [{
                                "dimension": "conversation",
                                "scope_id": "conversation:rachel",
                                "occurred_at": "2026-04-12T15:05:00Z",
                                "sequence": 2,
                                "valid_from": "2026-04-12T15:05:00Z",
                                "valid_until": "2026-04-12T18:00:00Z",
                                "rank": 2
                            }]
                        },
                        {
                            // A window that is still open where DETAIL's has
                            // closed. Rewinding the validity clock from here
                            // is what makes DETAIL expired rather than absent,
                            // and `expired_json` is reachable no other way.
                            "id": CURRENT,
                            "kind": "claim",
                            "text": "The Boulder start date is the one that stands.",
                            "coordinates": [{
                                "dimension": "conversation",
                                "scope_id": "conversation:rachel",
                                "occurred_at": "2026-04-12T18:00:00Z",
                                "sequence": 3,
                                "valid_from": "2026-04-12T18:00:00Z"
                            }]
                        }
                    ],
                    "relations": [{
                        "from": DETAIL,
                        "to": CLAIM,
                        "rel": "supports",
                        "class": "evidential",
                        "confidence": "high",
                        "why": "The start date is what makes the move concrete.",
                        "evidence": "Both statements came from the same conversation."
                    }],
                    "evidence": [{
                        "id": "evidence:question:parity:denver",
                        "text": "Rachel named Denver twice in the same conversation.",
                        "source": "parity fixture",
                        "time": "2026-04-12T15:00:00Z",
                        "supports": [CLAIM]
                    }]
                }
            }),
        ),
        (
            "kmp_write_memory",
            json!({
                "about": ABOUT,
                "intent": "record_decision",
                "actor": "parity-test",
                "source_kind": "agent",
                "observed_at": "2026-04-12T16:00:00Z",
                "occurred_at": "2026-04-12T16:00:00Z",
                "scope": {"process": "parity"},
                "current": {
                    "kind": "decision",
                    "summary": "Pin the answered surface, not only the advertised one.",
                    "evidence": "A refactor that preserves schemas can still change answers."
                },
                "connect_to": [{
                    "ref": CLAIM,
                    "rel": "uses_background",
                    "class": "evidential",
                    "confidence": "medium",
                    "why": "The decision was taken while reading this claim back.",
                    "evidence": "The claim is the store's only prior entry."
                }],
                "read_context": {"inspected_refs": [CLAIM]}
            }),
        ),
        // `proof.superseded` and `proof.expired` are pinned empty by every
        // other call, so their element mappers — `superseded_by`, and the
        // expired `valid_until` — would have no assertion anywhere.
        // `superseded_by` is a published field: six advertised output schemas
        // name it. This write puts a real one in the store.
        (
            "kmp_write_memory:supersedes",
            json!({
                "about": ABOUT,
                "intent": "record_delta",
                "actor": "parity-test",
                "source_kind": "agent",
                "observed_at": "2026-04-12T17:00:00Z",
                "occurred_at": "2026-04-12T17:00:00Z",
                "scope": {"process": "parity"},
                "current": {
                    "kind": "semantic_delta",
                    "summary": "Rachel is moving to Boulder, not Denver.",
                    "evidence": "She corrected the city in the same conversation."
                },
                "semantic_delta": {
                    "from": "Rachel said she was moving to Denver.",
                    "to": "Rachel is moving to Boulder.",
                    "why": "The later statement corrects the earlier one.",
                    "evidence": "Both statements are hers, minutes apart."
                },
                "connect_to": [{
                    "ref": CLAIM,
                    "rel": "supersedes",
                    "class": "evidential",
                    "confidence": "high",
                    "why": "The corrected city replaces the first one while the \
            first stays readable as history.",
                    "evidence": "She named Boulder after naming Denver."
                }],
                "read_context": {"inspected_refs": [CLAIM]}
            }),
        ),
        ("kmp_wake", json!({"about": ABOUT})),
        (
            "kmp_ask",
            json!({"about": ABOUT, "question": "Where is Rachel moving?"}),
        ),
        (
            "kmp_ask:trimmed",
            json!({
                "about": ABOUT,
                "question": "Where is Rachel moving?",
                "budget": {"detail": "compact", "max_bytes": 6000}
            }),
        ),
        (
            "kmp_goto",
            // `limit` forces a partial page, so the continuation guidance is
            // a real string rather than the null every unbounded read pins.
            // Each direction words it differently, so each needs its own call.
            json!({
                "about": ABOUT,
                "at": {"time": "2026-04-12T15:05:00Z"},
                "axis": "occurred",
                "limit": {"entries": 1},
                // A non-default dimension selection, so `coverage.requested`
                // carries more than its two default keys.
                "dimensions": {
                    "mode": "only",
                    "scope": "abouts",
                    "abouts": [ABOUT],
                    "include": ["conversation"]
                }
            }),
        ),
        (
            "kmp_near",
            json!({
                "about": ABOUT,
                "around": {"time": "2026-04-12T15:00:00Z"},
                "axis": "occurred",
                "window": {"before_entries": 2, "after_entries": 2},
                // `scope_ids` is the one selection key no other call fills,
                // and near takes no `limit`, so naming it here cannot hide a
                // continuation the way it did on forward.
                // Supersession is derived from the relations among the
                // *selected* entries, so this call deliberately spans every
                // dimension: filtering to one would drop the delta that
                // supersedes the first claim. The temporal path's element
                // mapper is a separate function from the recall path's, and
                // nothing else reaches it.
                "include": {"relations": true, "evidence": true},
                // Relations and evidence do not fit the 10,000-byte default.
                "budget": {"max_bytes": 40000}
            }),
        ),
        (
            "kmp_rewind",
            json!({
                "about": ABOUT,
                "from": {"time": "2026-04-12T16:00:00Z"},
                "axis": "occurred",
                "limit": {"entries": 1}
            }),
        ),
        (
            "kmp_forward",
            json!({
                "about": ABOUT,
                "from": {"time": "2026-04-12T15:00:00Z"},
                "axis": "occurred",
                "limit": {"entries": 1},
                // `except` is the one selection mode no other call reaches.
                // `except` is the one selection mode no other call reaches.
                // `scope_ids` deliberately lives on kmp_near instead: a filter
                // narrow enough to fill it here would leave one entry, cancel
                // the partial page and take the continuation guidance with it.
                "dimensions": {"mode": "except", "exclude": ["task"]}
            }),
        ),
        // `proof.expired` stays empty on every other axis, so `expired_json`
        // — the mapper for an entry whose applicability ended — has no
        // assertion anywhere. The ingest above writes a valid_until; reading
        // the validity clock is what surfaces it.
        (
            "kmp_rewind:validity",
            json!({
                "about": ABOUT,
                "from": {"ref": CURRENT},
                "axis": "validity",
                // `scope_ids` is the one selection key no other call fills.
                // It lives here because this call needs no partial page and
                // no cross-dimension selection, so narrowing costs nothing.
                "dimensions": {
                    "mode": "only",
                    "include": ["conversation"],
                    "scope_ids": ["about:question:parity:dimension:conversation:rachel"]
                }
            }),
        ),
        (
            "kmp_inspect",
            json!({"about": ABOUT, "ref": CLAIM, "include": {"raw": true}}),
        ),
        // A second inspect under a ceiling small enough to page. Without it
        // the inspect budget — a whole file this refactor lifted out — is
        // reached only on its everything-fits branch, leaving its guidance,
        // its cursor and the `quality.truncated` it recomputes unasserted.
        (
            "kmp_inspect:paged",
            json!({
                "about": ABOUT,
                "ref": CLAIM,
                // Deliberately without `raw`. The cursor is a hash over the
                // items it pages, and a raw record carries `content_hash`,
                // which embeds the ingest clock — including it would make the
                // cursor differ on every run. The unpaged call above pins the
                // raw record instead.
                "budget": {"max_bytes": 2500}
            }),
        ),
        (
            "kmp_trace",
            json!({"about": ABOUT, "from": DETAIL, "to": CLAIM, "budget": {"depth": 3}}),
        ),
        ("kmp_view_open", json!({"about": ABOUT})),
        (
            "kmp_view_apply_intent",
            json!({
                "idempotency_key": "parity:view:1",
                "explanation": "parity test frames the claim",
                "projection": {"semantic_zoom": "atlas"},
                "selection": CLAIM
            }),
        ),
        ("kmp_view_get_state", json!({})),
        // App-only, and reachable only after `initialize` negotiates MCP Apps.
        // `visual_projection_from_response` is their sole mapper and lives in
        // the file this refactor splits, so leaving them unpinned would leave
        // that mapper unguarded.
        // All three levels of detail, because each fills a different part of
        // `visual_projection_from_response` and leaves the rest empty:
        // `atlas` bins only, `episode` is the sole producer of `clusters`
        // (kmp-application/src/memory/visual_projection.rs:279), and `moment`
        // is the sole producer of `entries` and `relations` (:286, :305).
        // Pinning one level would leave most of that mapper asserted by an
        // empty array.
        (
            "kmp_view_read_projection",
            json!({
                "about": ABOUT,
                "axis": "occurred",
                "lod": "moment",
                "from": "2026-04-12T00:00:00Z",
                "to": "2026-04-13T00:00:00Z"
            }),
        ),
        (
            "kmp_view_read_projection:episode",
            json!({
                "about": ABOUT,
                "axis": "occurred",
                "lod": "episode",
                "from": "2026-04-12T00:00:00Z",
                "to": "2026-04-13T00:00:00Z"
            }),
        ),
        (
            "kmp_view_read_projection:atlas",
            json!({
                "about": ABOUT,
                "axis": "occurred",
                "lod": "atlas",
                "from": "2026-04-12T00:00:00Z",
                "to": "2026-04-13T00:00:00Z"
            }),
        ),
        ("kmp_view_undo", json!({})),
    ]
}

fn responses_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/contract/responses")
}

fn templates_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/contract/templates")
}

fn blessing() -> bool {
    std::env::var_os(BLESS).is_some()
}

/// What the kernel derives from the wall clock, which therefore cannot be
/// pinned:
///
/// - `at` — when a view intent landed.
/// - `ingested_at` — when the store learned a coordinate.
/// - `content_hash` — on a raw record. It is not purely content-addressed:
///   `ingested_at` is inside what it digests, so it moves between two runs
///   that stored identical text. Only the digest is dropped; the rest of the
///   raw record — ref, kind, revision, text, detail, coordinates — stays
///   pinned.
///
/// Everything else stays pinned. `occurred_at` and `observed_at` are
/// deliberately *not* redacted: they come from the calls above, and a refactor
/// that lost a clock is exactly what this test exists to catch.
///
/// Rendered answers embed their structured content as a JSON string, so the
/// same values are replaced in there too.
/// `required_bytes` is the serialized length of the whole answer, and that
/// answer contains `ingested_at` — whose `Display` emits 0, 3, 6 or 9
/// fractional digits depending on the nanoseconds it happens to carry. So the
/// number moves by ten bytes at random, roughly one run in three hundred.
/// Masking the value keeps the key pinned: a rename still fails, only the
/// count stops being asserted. A flaky red would be worse than that, and a
/// bless taken under the flake would pin a number every later run rejects.
const VOLATILE_KEYS: [&str; 4] = ["at", "ingested_at", "content_hash", "required_bytes"];
const REDACTED: &str = "<stamped at call time>";

/// An inspect cursor is `kmpi1:<offset>:<sha256>`, and the digest covers the
/// links it pages — whose coordinates carry `ingested_at`. The version and the
/// offset are contract and stay pinned; only the digest is dropped. A temporal
/// `next_cursor` is an entry ref and is left alone.
fn redact_inspect_digest(text: &str) -> Option<String> {
    let (head, digest) = text.rsplit_once(':')?;
    let looks_like_a_digest =
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit());
    (head.starts_with("kmpi") && looks_like_a_digest).then(|| format!("{head}:{REDACTED}"))
}

fn redact(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (key, child) in fields.iter_mut() {
                if VOLATILE_KEYS.contains(&key.as_str()) && !child.is_object() {
                    *child = json!(REDACTED);
                } else if let Some(text) = child.as_str()
                    && let Some(masked) = redact_inspect_digest(text)
                {
                    *child = json!(masked);
                } else {
                    redact(child);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact),
        // The rendered text block embeds structured content as a JSON string.
        // Redact inside it *textually*: re-serializing would normalize the
        // formatting and key order of the one field a host without structured
        // content puts into model context, and a change to how that block is
        // rendered — the compact-versus-pretty regression `protocol::result`
        // warns about — would then pin identically either way.
        Value::String(text) => {
            redact_embedded_text(text);
            redact_interpolated_byte_count(text);
        }
        _ => {}
    }
}

/// The shape of an answer with its data removed: every key kept, every scalar
/// replaced by its type, every array reduced to one representative element.
///
/// Pinned beside the instance because the two fail for different reasons. An
/// instance drifts whenever the calls above write different text, which makes
/// it noisy to review; a template drifts only when a verb gains, loses or
/// retypes a field — which is the thing a refactor must not do.
fn shape(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, child)| (key.clone(), shape(child)))
                .collect(),
        ),
        Value::Array(items) => match items.first() {
            None => json!([]),
            Some(first) => json!([shape(first)]),
        },
        Value::String(text) => {
            // Structured content is embedded as a JSON string; shape what is
            // inside it rather than calling the whole thing "string".
            match serde_json::from_str::<Value>(text) {
                Ok(embedded) if embedded.is_object() => shape(&embedded),
                _ => json!("string"),
            }
        }
        Value::Number(number) => json!(if number.is_i64() || number.is_u64() {
            "integer"
        } else {
            "number"
        }),
        Value::Bool(_) => json!("boolean"),
        Value::Null => json!("null"),
    }
}

/// Replace `"<key>": "<value>"` in raw JSON text, leaving every other byte —
/// spacing, escaping, key order — exactly as the server wrote it.
/// The inspect guidance interpolates `required_bytes` into its prose, so the
/// same wall-clock wobble reaches the sentence as well as the field. Mask the
/// digits and keep every other word, so the wording itself stays pinned.
fn redact_interpolated_byte_count(text: &mut String) {
    let needle = "requires ";
    let Some(found) = text.find(needle) else {
        return;
    };
    let start = found + needle.len();
    let digits = text[start..].bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        text.replace_range(start..start + digits, REDACTED);
    }
}

fn redact_embedded_text(text: &mut String) {
    if !matches!(serde_json::from_str::<Value>(text), Ok(value) if value.is_object()) {
        return;
    }
    for key in VOLATILE_KEYS {
        let needle = format!("\"{key}\":");
        let mut from = 0;
        while let Some(found) = text[from..].find(&needle) {
            let after_key = from + found + needle.len();
            let rest = &text[after_key..];
            let value_start = after_key + rest.len() - rest.trim_start().len();
            if !text[value_start..].starts_with('"') {
                from = after_key;
                continue;
            }
            let Some(closing) = text[value_start + 1..].find('"') else {
                return;
            };
            let value_end = value_start + 1 + closing + 1;
            let replacement = format!("\"{REDACTED}\"");
            text.replace_range(value_start..value_end, &replacement);
            from = value_start + replacement.len();
        }
    }
}

fn pin(path: &Path, actual: &Value, what: &str) {
    if blessing() {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create fixture dir");
        let mut text = serde_json::to_string_pretty(actual).expect("serializes");
        text.push('\n');
        std::fs::write(path, text).expect("write fixture");
        return;
    }

    let relative = path
        .strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .unwrap_or(path)
        .display();
    let raw = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("missing {relative}: {error}; regenerate with {BLESS}=1"));
    let expected: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("{relative} is not JSON: {error}"));

    assert!(
        *actual == expected,
        "{what} drifted from {relative}. A refactor must not change it. Regenerate with \
         {BLESS}=1 and read the diff to see what moved."
    );
}

#[test]
fn the_advertised_tool_definitions_match_their_reviewed_fixture() {
    let contract = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/contract");
    pin(
        &contract.join("tools_list.json"),
        &kmp_mcp::kmp_mcp_tools_list_result(),
        "the advertised tool definitions",
    );

    // A host that negotiates MCP Apps is offered a different document: two
    // extra tools and a `_meta.ui` block patched onto `kmp_view_open`. It is
    // assembled by mutating the surface above, so pinning only that one would
    // leave the mutation — and the argument rejection that reads the app
    // schemas — free to drift.
    pin(
        &contract.join("tools_list_with_apps.json"),
        &kmp_mcp::kmp_mcp_tools_list_result_with_apps(true),
        "the advertised tool definitions with MCP Apps negotiated",
    );
}

#[tokio::test]
async fn every_tool_answers_what_its_reviewed_fixture_says() {
    let store = tempfile::tempdir().expect("temporary store");
    let backend = EmbeddedKernelMcpBackend::open(store.path()).expect("embedded backend");
    let server = KernelMcpServer::with_embedded_backend(backend);

    // Negotiate MCP Apps, so the two app-only tools are callable and the
    // surface under test is the full fifteen rather than the thirteen a plain
    // host sees.
    server
        .handle_json_line(
            &json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {"capabilities": {"extensions": {
                    "io.modelcontextprotocol/ui": {"mimeTypes": [APP_MIME]}
                }}}
            })
            .to_string(),
        )
        .await
        .expect("initialize answers");

    for (label, arguments) in calls() {
        let tool = label.split(':').next().expect("tool name");
        let raw = server
            .handle_json_line(
                &json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": {"name": tool, "arguments": arguments}
                })
                .to_string(),
            )
            .await
            .unwrap_or_else(|| panic!("{label} produced no response"));
        let response: Value = serde_json::from_str(&raw).expect("the server answers JSON-RPC");

        assert!(
            response.get("error").is_none(),
            "{label} failed before it could be pinned: {response}"
        );
        assert_ne!(
            response["result"]["isError"], true,
            "{label} answered with a tool error, which pins nothing useful: {response}"
        );

        let file = format!("{}.json", label.replace(':', "-"));
        let mut result = response["result"].clone();
        redact(&mut result);
        pin(&responses_dir().join(&file), &result, label);
        pin(
            &templates_dir().join(&file),
            &shape(&result),
            &format!("the shape of {label}"),
        );
    }
}

/// A blessed set is only worth pinning while it covers the whole surface.
/// Without this, a build that lost a tool would bless a smaller set and pass
/// forever after.
#[test]
fn the_pinned_calls_cover_every_advertised_tool() {
    // The apps surface, not the plain one: comparing against the smaller list
    // is how the two app-only tools went unpinned while this guard passed.
    let surface = kmp_mcp::kmp_mcp_tools_list_result_with_apps(true);
    let advertised = surface["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("name").to_string())
        .collect::<Vec<_>>();
    assert_eq!(advertised.len(), 15, "advertised tools: {advertised:?}");

    for tool in &advertised {
        assert!(
            calls()
                .iter()
                .any(|(label, _)| label.split(':').next() == Some(tool.as_str())),
            "{tool} is advertised but never called, so nothing pins its answer"
        );
    }
}
