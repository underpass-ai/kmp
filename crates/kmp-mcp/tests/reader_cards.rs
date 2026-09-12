//! What a compact or bounded trace actually puts on the wire, over a real
//! embedded store through the real tool surface.
//!
//! Every body here carries a marker that appears nowhere else, so a response
//! is searched for the marker as a whole document rather than in the one field
//! a byte counter happens to watch. That is the check that matters: a body can
//! be withheld from `objects[].text` and still arrive through another field,
//! and then every counter reports a saving the caller never received.

use kmp_mcp::{EmbeddedKernelMcpBackend, KernelMcpServer};
use serde_json::{Value, json};

const ABOUT: &str = "question:cards";
const ENTRY_A: &str = "question:cards:claim:a";
const ENTRY_B: &str = "question:cards:claim:b";
const SOURCE: &str = "evidence:question:cards:claim:a:shared";
const SOURCE_MARKER: &str = "ZQSHAREDSOURCEBODY";
const ENTRY_MARKER: &str = "ZQENTRYBODY";

fn long_body(marker: &str) -> String {
    format!("{marker} {}", "filler ".repeat(120))
}

async fn call(server: &KernelMcpServer, name: &str, arguments: Value) -> Value {
    let raw = server
        .handle_json_line(
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
                    "params":{"name":name,"arguments":arguments}})
            .to_string(),
        )
        .await
        .expect("a response");
    serde_json::from_str(&raw).expect("JSON-RPC")
}

fn structured(response: &Value) -> &Value {
    assert_ne!(
        response["result"]["isError"], true,
        "unexpected tool error: {response}"
    );
    &response["result"]["structuredContent"]
}

/// The whole response as one string. A marker found anywhere in it was
/// delivered, whatever field carried it.
fn wire(response: &Value) -> String {
    response.to_string()
}

/// How many times a marker appears in one response.
fn occurrences(response: &Value, marker: &str) -> usize {
    wire(response).matches(marker).count()
}

/// The proof objects as one string: the body delivery, without the relation
/// explanations that the agreed scope keeps intact.
fn objects_wire(response: &Value) -> String {
    structured(response)["objects"].to_string()
}

async fn seeded() -> (tempfile::TempDir, KernelMcpServer) {
    let dir = tempfile::tempdir().expect("store");
    let backend = EmbeddedKernelMcpBackend::open(dir.path()).expect("backend");
    let server = KernelMcpServer::with_embedded_backend(backend);
    let response = call(
        &server,
        "kmp_ingest",
        json!({
            "about": ABOUT,
            "idempotency_key": "ingest:cards",
            "memory": {
                "dimensions": [{"id": "conversation:s1", "kind": "conversation"}],
                "entries": [
                    {"id": ENTRY_A, "kind": "claim", "text": long_body(ENTRY_MARKER),
                     "coordinates": [{"dimension":"conversation","scope_id":"conversation:s1",
                                      "occurred_at":"2026-07-22T10:00:00Z","sequence":1}]},
                    {"id": ENTRY_B, "kind": "claim", "text": long_body(ENTRY_MARKER),
                     "coordinates": [{"dimension":"conversation","scope_id":"conversation:s1",
                                      "occurred_at":"2026-07-22T10:00:01Z","sequence":2}]}
                ],
                "relations": [{"from": ENTRY_A, "to": ENTRY_B, "rel": "depends_on",
                               "class": "causal", "confidence": "high",
                               "why": "A depends on B in this fixture.",
                               "evidence": "reader card wire test"}],
                "evidence": [{"id": SOURCE, "supports": [ENTRY_A, ENTRY_B],
                              "text": long_body(SOURCE_MARKER),
                              "source": "reader card wire test"}]
            }
        }),
    )
    .await;
    assert_ne!(response["result"]["isError"], true, "seed: {response}");
    (dir, server)
}

fn trace(search: Value) -> Value {
    json!({
        "about": ABOUT,
        "from": ENTRY_A,
        "to": ENTRY_B,
        "search": search,
        "budget": {"max_bytes": 400_000}
    })
}

/// Reads the descriptor of one ref from a descriptor-only trace.
async fn descriptor_of(server: &KernelMcpServer, node_id: &str) -> Value {
    let response = call(
        server,
        "kmp_trace",
        trace(json!({"proof": true, "proof_refs": []})),
    )
    .await;
    structured(&response)["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .find(|object| object["ref"] == node_id)
        .unwrap_or_else(|| panic!("`{node_id}` must stay in the response"))["descriptor"]
        .clone()
}

async fn condense(server: &KernelMcpServer, node_id: &str, card: &str) -> Value {
    let descriptor = descriptor_of(server, node_id).await;
    call(
        server,
        "kmp_condense",
        json!({
            "about": ABOUT,
            "ref": node_id,
            "language": "es",
            "scope": "node_body",
            "card": card,
            "source": {
                "revision": descriptor["revision"],
                "content_hash": descriptor["content_hash"],
                "record_digest": descriptor["record_digest"]
            },
            "expect": {"absent": true},
            "actor": "reader-card-test"
        }),
    )
    .await
}

#[tokio::test]
async fn a_descriptor_only_read_puts_no_canonical_body_on_the_wire() {
    let (_dir, server) = seeded().await;

    let full = call(&server, "kmp_trace", trace(json!({"proof": true}))).await;
    assert!(
        wire(&full).contains(SOURCE_MARKER),
        "the unbounded read does carry the body, which is what makes the next assertion mean something"
    );

    let descriptors = call(
        &server,
        "kmp_trace",
        trace(json!({"proof": true, "proof_refs": []})),
    )
    .await;

    assert!(
        !objects_wire(&descriptors).contains(SOURCE_MARKER),
        "a withheld source body reached the wire as a proof object anyway"
    );
    assert!(
        !wire(&descriptors).contains(ENTRY_MARKER),
        "a withheld entry body reached the wire anyway"
    );
    // Declared residual, not a leak the scope forbids: the writer copies a
    // source body into the explanation of every relation it supports, and
    // this increment keeps explanations whole. It is counted here so the cost
    // is visible rather than denied.
    let residual = occurrences(&descriptors, SOURCE_MARKER);
    assert_eq!(
        residual,
        occurrences(&full, SOURCE_MARKER) - 1,
        "exactly the body copy is gone; every relation explanation remains"
    );
    let value = structured(&descriptors);
    assert!(
        value["proof"]["manifest_id"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "and the caller still gets the identity it needs to expand: {value}"
    );
    for object in value["objects"].as_array().expect("objects") {
        assert_eq!(object["body_state"], "not_requested");
        assert!(
            object["descriptor"]["record_bytes"].as_u64().is_some(),
            "every withheld body keeps its identity and cost: {object}"
        );
        assert!(
            object.get("text").is_none(),
            "no empty text to mistake for an empty body: {object}"
        );
    }
}

#[tokio::test]
async fn a_compact_read_carries_the_card_and_not_the_body() {
    let (_dir, server) = seeded().await;
    for node_id in [ENTRY_A, ENTRY_B, SOURCE] {
        let written = condense(&server, node_id, "Tarjeta corta del lector.").await;
        assert_ne!(written["result"]["isError"], true, "{written}");
    }

    let compact = call(
        &server,
        "kmp_trace",
        trace(json!({"proof": true, "compact": {"language": "es"}})),
    )
    .await;

    assert!(
        !objects_wire(&compact).contains(SOURCE_MARKER),
        "the source body was delivered as a proof object"
    );
    assert!(
        !wire(&compact).contains(ENTRY_MARKER),
        "an entry body was delivered"
    );
    assert!(
        wire(&compact).contains("Tarjeta corta del lector."),
        "the card the reader wrote is what stands in its place"
    );
    let value = structured(&compact);
    let summary = &value["proof"]["compact"];
    assert_eq!(summary["stale"], 0);
    assert_eq!(summary["absent"], 0);
    assert!(summary["body_bytes_omitted"].as_u64().expect("omitted") > 0);
    for object in value["objects"].as_array().expect("objects") {
        assert_eq!(object["body_state"], "compact");
        assert_eq!(object["card"]["status"], "valid");
    }
}

#[tokio::test]
async fn a_named_expansion_returns_the_body_it_named_and_no_other() {
    let (_dir, server) = seeded().await;
    let first = call(
        &server,
        "kmp_trace",
        trace(json!({"proof": true, "proof_refs": []})),
    )
    .await;
    let manifest = structured(&first)["proof"]["manifest_id"]
        .as_str()
        .expect("manifest")
        .to_string();

    let expansion = call(
        &server,
        "kmp_trace",
        trace(json!({
            "proof": true,
            "max_body_record_bytes": 400_000,
            "proof_refs": [SOURCE],
            "expect_selection": manifest
        })),
    )
    .await;

    let objects = objects_wire(&expansion);
    assert!(
        objects.contains(SOURCE_MARKER),
        "the body that was named must come back"
    );
    assert!(
        !wire(&expansion).contains(ENTRY_MARKER),
        "and nothing that was not named: {objects}"
    );
}

#[tokio::test]
async fn a_refused_expansion_carries_no_text_of_the_selection_it_refused() {
    let (_dir, server) = seeded().await;

    let refused = call(
        &server,
        "kmp_trace",
        trace(json!({
            "proof": true,
            "max_body_record_bytes": 400_000,
            "proof_refs": [SOURCE],
            "expect_selection": "a manifest this store never produced"
        })),
    )
    .await;

    let text = wire(&refused);
    assert!(!text.contains(SOURCE_MARKER), "refused and still delivered");
    assert!(!text.contains(ENTRY_MARKER), "refused and still delivered");
    // The refusal is the one place where nothing of the selection survives,
    // explanations included.
    assert!(
        !text.contains("A depends on B in this fixture."),
        "a relation's own why is text of the refused selection too: {text}"
    );
    let value = structured(&refused);
    assert_eq!(value["expansion_refusal"]["code"], "read_selection_changed");
    assert_eq!(value["trace"], json!([]));
    assert_eq!(value["objects"], json!([]));
    let fresh = &value["expansion_refusal"]["fresh_read"];
    assert_eq!(fresh["tool"], "kmp_trace");
    assert_eq!(
        fresh["arguments"]["search"]["expect_selection"],
        Value::Null
    );

    // The offered action is executable and returns a usable manifest.
    let again = call(&server, "kmp_trace", fresh["arguments"].clone()).await;
    assert!(
        structured(&again)["proof"]["manifest_id"]
            .as_str()
            .is_some_and(|manifest| !manifest.is_empty()),
        "the fresh read hands back the identity the refusal could not: {again}"
    );
}

#[tokio::test]
async fn the_offered_expansion_action_recovers_exactly_what_was_withheld() {
    let (_dir, server) = seeded().await;
    let first = call(
        &server,
        "kmp_trace",
        trace(json!({"proof": true, "proof_refs": []})),
    )
    .await;
    let action = structured(&first)["proof"]["expand_bodies"].clone();
    assert_eq!(action["tool"], "kmp_trace", "an action must be offered");

    let expanded = call(&server, "kmp_trace", action["arguments"].clone()).await;

    let objects = objects_wire(&expanded);
    assert!(objects.contains(SOURCE_MARKER));
    assert!(objects.contains(ENTRY_MARKER));
    for object in structured(&expanded)["objects"]
        .as_array()
        .expect("objects")
    {
        assert_eq!(
            object["body_state"], "loaded",
            "the action named every ref that was pending: {object}"
        );
    }
}

#[tokio::test]
async fn condense_over_the_tool_surface_holds_its_compare_and_set() {
    let (_dir, server) = seeded().await;
    let descriptor = descriptor_of(&server, SOURCE).await;

    let first = condense(&server, SOURCE, "La fuente sostiene ambas entradas.").await;
    let card = &structured(&first)["card"];
    assert_eq!(card["card_revision"], 1);
    assert_eq!(card["status"], "valid");
    assert_eq!(card["source_record_digest"], descriptor["record_digest"]);
    assert!(
        card["authored_at"]
            .as_str()
            .is_some_and(|at| !at.is_empty()),
        "the kernel stamps authorship, the caller does not: {card}"
    );

    // A second first-write loses, and says which revision is there.
    let replayed = condense(&server, SOURCE, "Otro resumen.").await;
    assert_eq!(replayed["result"]["isError"], true);
    assert_eq!(
        replayed["result"]["structuredContent"]["error"]["code"],
        "conflict"
    );

    // Declaring the stored revision replaces it.
    let replaced = call(
        &server,
        "kmp_condense",
        json!({
            "about": ABOUT, "ref": SOURCE, "language": "es", "scope": "node_body",
            "card": "Resumen corregido.",
            "source": {"revision": descriptor["revision"],
                       "record_digest": descriptor["record_digest"]},
            "expect": {"card_revision": 1},
            "actor": "reader-card-test"
        }),
    )
    .await;
    assert_eq!(structured(&replaced)["card"]["card_revision"], 2);

    // A body version this store does not hold is refused, not written against
    // whatever happens to be current.
    let stale = call(
        &server,
        "kmp_condense",
        json!({
            "about": ABOUT, "ref": SOURCE, "language": "es", "scope": "node_body",
            "card": "Resumen viejo.",
            "source": {"revision": descriptor["revision"],
                       "record_digest": format!("sha256:{}", "0".repeat(64))},
            "expect": {"card_revision": 2},
            "actor": "reader-card-test"
        }),
    )
    .await;
    assert_eq!(stale["result"]["isError"], true);
    assert_eq!(
        stale["result"]["structuredContent"]["error"]["code"],
        "conflict"
    );

    // And the canonical body is untouched by all of it.
    assert_eq!(
        descriptor_of(&server, SOURCE).await,
        descriptor,
        "condensing moves no canonical row"
    );
}

#[tokio::test]
async fn a_named_expansion_with_compact_delivers_the_body_and_not_its_card_again() {
    // F3: when both could be emitted, only one is. A card beside the body it
    // stands for is the same text twice, and a counter that ignored those
    // bytes would under-report what the response cost.
    let (_dir, server) = seeded().await;
    for node_id in [ENTRY_A, ENTRY_B, SOURCE] {
        condense(&server, node_id, "Tarjeta corta del lector.").await;
    }
    let first = call(
        &server,
        "kmp_trace",
        trace(json!({"proof": true, "proof_refs": []})),
    )
    .await;
    let manifest = structured(&first)["proof"]["manifest_id"]
        .as_str()
        .expect("manifest")
        .to_string();

    let expansion = call(
        &server,
        "kmp_trace",
        trace(json!({
            "proof": true,
            "max_body_record_bytes": 400_000,
            "proof_refs": [SOURCE],
            "expect_selection": manifest,
            "compact": {"language": "es"}
        })),
    )
    .await;

    let value = structured(&expansion);
    let expanded = value["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .find(|object| object["ref"] == SOURCE)
        .expect("source");
    assert_eq!(expanded["body_state"], "loaded");
    assert!(
        expanded["text"]
            .as_str()
            .is_some_and(|text| text.contains(SOURCE_MARKER)),
        "the body it named is delivered: {expanded}"
    );
    assert_eq!(
        expanded["card"]["status"], "valid",
        "the card's state is still reported"
    );
    assert!(
        expanded["card"].get("text").is_none(),
        "but not its prose, which would be the same text twice: {expanded}"
    );
    // The omitted total counts the bodies cards actually stood for, and only
    // those: the one this call expanded is delivered, not saved.
    let omitted_by_cards: u64 = value["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .filter(|object| object["body_state"] == "compact")
        .map(|object| object["descriptor"]["body_bytes"].as_u64().expect("bytes"))
        .sum();
    assert_eq!(
        value["proof"]["compact"]["body_bytes_omitted"]
            .as_u64()
            .expect("omitted"),
        omitted_by_cards,
        "the expanded body is delivered, so it is not counted as omitted"
    );
    assert!(
        omitted_by_cards > 0,
        "and the other two cards did save their bodies"
    );
}

#[tokio::test]
async fn following_the_offered_actions_recovers_every_body_exactly_once() {
    // One record at a time, over the real surface, until the chain ends. A
    // single executed action could not have caught a cycle.
    let (_dir, server) = seeded().await;
    let smallest = {
        let descriptors = call(
            &server,
            "kmp_trace",
            trace(json!({"proof": true, "proof_refs": []})),
        )
        .await;
        structured(&descriptors)["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .map(|object| object["required_record_bytes"].as_u64().expect("cost"))
            .max()
            .expect("at least one body")
    };

    let mut arguments = trace(json!({
        "proof": true,
        "max_body_record_bytes": smallest,
        "proof_refs": []
    }));
    let mut recovered: Vec<String> = Vec::new();
    let mut rounds = 0;

    loop {
        rounds += 1;
        assert!(rounds <= 12, "the chain did not finish");
        let response = call(&server, "kmp_trace", arguments.clone()).await;
        let value = structured(&response);
        for object in value["objects"].as_array().expect("objects") {
            if object["body_state"] == "loaded" {
                let reference = object["ref"].as_str().expect("ref").to_string();
                assert!(
                    !recovered.contains(&reference),
                    "`{reference}` was delivered twice across the chain"
                );
                recovered.push(reference);
            }
        }
        let Some(action) = value["proof"]["expand_bodies"].as_object() else {
            break;
        };
        arguments = action["arguments"].clone();
    }

    let mut once = recovered.clone();
    once.sort();
    once.dedup();
    assert_eq!(once.len(), recovered.len());
    assert!(
        recovered.contains(&SOURCE.to_string())
            && recovered.contains(&ENTRY_A.to_string())
            && recovered.contains(&ENTRY_B.to_string()),
        "the chain reached every body of the selection: {recovered:?}"
    );
}

const WIDE_ABOUT: &str = "question:wide";
const WIDE_COUNT: usize = 130;

fn wide_ref(index: usize) -> String {
    format!("{WIDE_ABOUT}:claim:{index:03}")
}

/// A chain longer than one expansion batch and longer than one page, so a
/// response's page can hold none of the refs its actions must name.
async fn seeded_wide() -> (tempfile::TempDir, KernelMcpServer) {
    let dir = tempfile::tempdir().expect("store");
    let backend = EmbeddedKernelMcpBackend::open(dir.path()).expect("backend");
    let server = KernelMcpServer::with_embedded_backend(backend);
    let entries: Vec<Value> = (0..WIDE_COUNT)
        .map(|index| {
            json!({"id": wide_ref(index), "kind": "claim",
                   "text": format!("{ENTRY_MARKER}-{index:03} {}", "filler ".repeat(8)),
                   "coordinates": [{"dimension":"conversation","scope_id":"conversation:s1",
                                    "occurred_at":"2026-07-22T10:00:00Z","sequence": index + 1}]})
        })
        .collect();
    let relations: Vec<Value> = (0..WIDE_COUNT - 1)
        .map(|index| {
            json!({"from": wide_ref(index), "to": wide_ref(index + 1), "rel": "depends_on",
                   "class": "causal", "confidence": "high",
                   "why": "The chain declares this link.",
                   "evidence": "wide chain fixture"})
        })
        .collect();
    let response = call(
        &server,
        "kmp_ingest",
        json!({
            "about": WIDE_ABOUT,
            "idempotency_key": "ingest:wide",
            "memory": {
                "dimensions": [{"id": "conversation:s1", "kind": "conversation"}],
                "entries": entries,
                "relations": relations,
                "evidence": [{"id": format!("evidence:{WIDE_ABOUT}:one"),
                              "supports": [wide_ref(0), wide_ref(WIDE_COUNT - 1)],
                              "text": long_body(SOURCE_MARKER),
                              "source": "wide chain fixture"}]
            }
        }),
    )
    .await;
    assert_ne!(response["result"]["isError"], true, "seed: {response}");
    (dir, server)
}

fn wide_trace(mut search: Value, page_entries: u64) -> Value {
    let options = search.as_object_mut().expect("search");
    options.insert("max_depth".into(), json!(1024));
    options.insert("max_nodes".into(), json!(4096));
    options.insert("max_edges".into(), json!(8192));
    options.insert("max_states".into(), json!(32768));
    json!({
        "about": WIDE_ABOUT,
        "from": wide_ref(0),
        "to": wide_ref(WIDE_COUNT - 1),
        "search": search,
        "page": {"entries": page_entries},
        "budget": {"max_bytes": 900_000}
    })
}

/// Every action a response offers, collected without looking at its objects.
fn offered(value: &Value) -> Vec<Value> {
    ["expand_bodies", "expand_oversized_body"]
        .iter()
        .filter_map(|name| value["proof"][*name]["arguments"].as_object().cloned())
        .map(Value::Object)
        .collect()
}

/// One operation, read to its last page: every ref it delivered and every
/// action any of its pages offered.
///
/// Walking the continuations is the point. An action is not allowed to depend
/// on which page it happened to land on, and a consumer that stops at the
/// first page must not lose refs the later ones carry.
async fn whole_operation(server: &KernelMcpServer, arguments: Value) -> (Vec<String>, Vec<Value>) {
    let mut loaded = Vec::new();
    let mut actions = Vec::new();
    let mut call_arguments = arguments;
    for _ in 0..40 {
        let response = call(server, "kmp_trace", call_arguments.clone()).await;
        let value = structured(&response);
        for object in value["objects"].as_array().expect("objects") {
            if object["body_state"] == "loaded" {
                loaded.push(object["ref"].as_str().expect("ref").to_string());
            }
        }
        actions.extend(offered(value));
        let Some(cursor) = value["page"]["next_cursor"].as_str() else {
            break;
        };
        call_arguments["page"] = json!({
            "entries": call_arguments["page"]["entries"].clone(),
            "cursor": cursor
        });
    }
    (loaded, actions)
}

#[tokio::test]
async fn following_every_offered_action_recovers_the_whole_selection_across_pages() {
    // The failure this pins: actions used to be derived from one page, so a
    // page holding none of the pending refs offered nothing and those bodies
    // were never named by any action. Following the whole protocol still lost
    // them. The page size changes between operations on purpose.
    let (_dir, server) = seeded_wide().await;
    // The whole selected table, gathered across every page of one read.
    let descriptor_only = wide_trace(json!({"proof": true, "proof_refs": []}), 32);
    let selected: Vec<String> = {
        let mut refs = Vec::new();
        let mut arguments = descriptor_only.clone();
        for _ in 0..40 {
            let response = call(&server, "kmp_trace", arguments.clone()).await;
            let value = structured(&response);
            for object in value["objects"].as_array().expect("objects") {
                refs.push(object["ref"].as_str().expect("ref").to_string());
            }
            let Some(cursor) = value["page"]["next_cursor"].as_str() else {
                break;
            };
            arguments["page"] = json!({"entries": 32, "cursor": cursor});
        }
        refs
    };
    assert!(
        selected.len() > 64,
        "the fixture must exceed one batch: {} objects",
        selected.len()
    );

    let (_, first_actions) = whole_operation(&server, descriptor_only).await;
    let mut pending = first_actions;
    assert!(
        !pending.is_empty(),
        "a descriptor-only read must offer an action"
    );
    let mut recovered: Vec<String> = Vec::new();
    let mut seen_arguments: Vec<Value> = Vec::new();
    let mut page_entries = 32;

    while let Some(mut arguments) = pending.pop() {
        if seen_arguments.contains(&arguments) {
            continue;
        }
        seen_arguments.push(arguments.clone());
        // A consumer is free to read the same selection with another page
        // size; recovery must not depend on the partition.
        arguments["page"] = json!({"entries": page_entries});
        page_entries = if page_entries == 32 { 64 } else { 32 };

        let (loaded, actions) = whole_operation(&server, arguments).await;
        for reference in loaded {
            assert!(
                !recovered.contains(&reference),
                "`{reference}` was delivered twice"
            );
            recovered.push(reference);
        }
        pending.extend(actions);
        assert!(
            seen_arguments.len() <= 400,
            "the chain is not terminating: {} calls",
            seen_arguments.len()
        );
    }

    let mut missing: Vec<&String> = selected
        .iter()
        .filter(|reference| !recovered.contains(reference))
        .collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "{} of {} selected bodies were never offered by any action: {:?}",
        missing.len(),
        selected.len(),
        &missing[..missing.len().min(8)]
    );
}
