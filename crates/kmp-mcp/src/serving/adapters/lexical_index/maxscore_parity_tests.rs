//! MaxScore against the floor (DESIGN L6, P14) is exact: over randomized
//! stores and questions, an ask answered from the index with the candidates
//! below the floor left unread gives the bytes reading the whole about gives,
//! first page and every continuation (kmp2 cursors), with and without the
//! lexical bridge, under both policies, and at a deeper depth.

use std::path::{Path, PathBuf};

use kmp_application::MemoryAnswerPolicy;
use kmp_application::memory::AskMemoryQuery;
use kmp_domain::{DimensionSelection, TemporalSelection};
use kmp_embedded::EmbeddedKernel;
use kmp_proto_mapping::v1beta1::{AskRetrievalContext, LexicalBridge, ask_response_from_result};
use serde_json::{Value, json};

use super::index_limits::IndexLimits;
use super::lexical_sidecar::LexicalSidecar;
use super::upkeep_tests::{ABOUT, call, write};
use crate::serving::lexical_index_mode::LexicalIndexMode;
use crate::{EmbeddedKernelMcpBackend, KernelMcpServer};

const AT: &str = "2026-09-01T10:00:00Z";
const LATER: &str = "2026-09-02T10:00:00Z";

/// Common words reach most memories, rare ones a few: the floor then
/// refuses memories that carry only common words.
const COMMON: [&str; 6] = ["plant", "shift", "crew", "report", "line", "check"];
const RARE: [&str; 14] = [
    "valve",
    "pump",
    "glycol",
    "seal",
    "frost",
    "boiler",
    "turbine",
    "sensor",
    "filter",
    "gasket",
    "compressor",
    "manifold",
    "bearing",
    "coolant",
];
const SPANISH: [&str; 4] = ["válvula", "bomba", "sensor", "junta"];

/// A deterministic generator (SplitMix64), so every run is the same store.
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a>(&mut self, words: &[&'a str]) -> &'a str {
        words[self.below(words.len())]
    }
}

fn server(path: &Path, mode: LexicalIndexMode) -> KernelMcpServer {
    KernelMcpServer::with_backend(
        EmbeddedKernelMcpBackend::open(path)
            .expect("store")
            .with_lexical_index(mode),
    )
}

fn shipped_bridge() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../distribution/lexical-bridge")
        .join("kmp-lexical-bridge.kmpb")
}

fn packet(key: &str, at: &str, memories: Value) -> Value {
    json!({"about":ABOUT,"actor":"fixture","source_kind":"human","idempotency_key":key,
        "occurred_at":at,"observed_at":at,"labels":{"component":["plant"]},
        "options":{"strict":false},"memories":memories})
}

/// One memory's sentence: common words, sometimes rare ones, repeated now
/// and then (tf above one), sometimes Spanish.
fn sentence(draw: &mut Draw, index: usize) -> String {
    let mut words = Vec::new();
    for _ in 0..(2 + draw.below(4)) {
        words.push(draw.pick(&COMMON).to_string());
    }
    for _ in 0..draw.below(3) {
        let word = draw.pick(&RARE);
        words.push(word.to_string());
        if draw.below(4) == 0 {
            words.push(word.to_string());
        }
    }
    if draw.below(6) == 0 {
        words.push(draw.pick(&SPANISH).to_string());
    }
    format!("Entry {index}: {}.", words.join(" "))
}

/// A randomized about: memories, supports between them, a replacement and
/// an expiry, written in two batches.
async fn seed(server: &KernelMcpServer, draw: &mut Draw, memories: usize) {
    let first = (0..memories)
        .map(|index| {
            let mut memory = json!({"id":format!("m{index}"),"kind":"observation",
                "summary":sentence(draw, index),
                "evidence":format!("Log {index}: {}", sentence(draw, index + memories))});
            if draw.below(15) == 0 {
                memory["valid_from"] = json!(AT);
                memory["valid_until"] = json!("2026-09-01T11:00:00Z");
            }
            memory
        })
        .collect::<Vec<_>>();
    let written = write(server, packet("seed:first", AT, json!(first))).await;
    let refs = &written["local_refs"];
    let later = (0..memories / 10)
        .map(|index| {
            let target = draw.below(memories);
            let rel = if draw.below(3) == 0 {
                "supersedes"
            } else {
                "supports"
            };
            json!({"id":format!("later{index}"),"kind":"decision",
                "summary":sentence(draw, index + 2 * memories),
                "evidence":format!("Order {index} follows up."),
                "connect_to":[{"ref":refs[format!("m{target}")],"rel":rel,"class":"evidential",
                    "why":format!("Follow-up {index} of entry {target}."),
                    "evidence":"The order names it.","confidence":"high"}]})
        })
        .collect::<Vec<_>>();
    write(server, packet("seed:later", LATER, json!(later))).await;
}

fn question(draw: &mut Draw) -> String {
    let mut words = vec![draw.pick(&COMMON), draw.pick(&RARE)];
    for _ in 0..draw.below(3) {
        words.push(if draw.below(2) == 0 {
            draw.pick(&RARE)
        } else {
            draw.pick(&COMMON)
        });
    }
    if draw.below(5) == 0 {
        words.push(draw.pick(&SPANISH));
    }
    format!("what about the {}?", words.join(" "))
}

fn query(question: &str, policy: MemoryAnswerPolicy, depth: u32) -> AskMemoryQuery {
    AskMemoryQuery {
        about: ABOUT.to_string(),
        question: question.to_string(),
        asked_as: None,
        answer_policy: policy,
        dimensions: DimensionSelection::default(),
        token_budget: 2400,
        depth,
        max_tier: None,
        max_entries: None,
        temporal: TemporalSelection::Frontier,
    }
}

/// Every randomized question answered from the index with MaxScore gives
/// the response reading the whole about gives, field for field; and the
/// floor did leave candidates unread.
async fn assert_pruned_asks_equal_the_about(seed_value: u64, bridge: LexicalBridge) {
    let mut draw = Draw(seed_value);
    let directory = tempfile::tempdir().expect("store");
    seed(
        &server(directory.path(), LexicalIndexMode::Off),
        &mut draw,
        90,
    )
    .await;
    let kernel = EmbeddedKernel::open(directory.path()).expect("kernel");
    let limits = IndexLimits {
        max_candidate_share_percent: 100,
        ..IndexLimits::DEFAULT.every_about()
    };
    let sidecar =
        LexicalSidecar::open(directory.path(), LexicalIndexMode::On, limits).with_maxscore(true);
    let service = kernel.service();
    let (mut asks, mut pruned) = (0, 0);
    for _ in 0..8 {
        let question = question(&mut draw);
        for (policy, depth) in [
            (MemoryAnswerPolicy::EvidenceOrUnknown, 2),
            (MemoryAnswerPolicy::BestEffort, 2),
            (MemoryAnswerPolicy::EvidenceOrUnknown, 3),
        ] {
            let query = query(&question, policy, depth);
            let followed = sidecar.catch_up(kernel.store(), Some(ABOUT)).await;
            let Ok(read) = sidecar
                .indexed_read(
                    kernel.store(),
                    &service,
                    &query,
                    followed.as_ref(),
                    &bridge,
                    depth > 2,
                )
                .await
                .expect("the index reads")
            else {
                continue;
            };
            asks += 1;
            pruned += usize::from(read.candidates < read.reached);
            let indexed = ask_response_from_result(
                &question,
                None,
                policy,
                None,
                AskRetrievalContext::from(read.result)
                    .with_default_gate()
                    .with_indexed(read.indexed),
                &bridge,
                &TemporalSelection::Frontier,
            )
            .expect("indexed answer");
            let whole = service
                .ask_on_demand(query, kmp_application::RenderDemand::Skip)
                .await
                .expect("ask");
            let about = ask_response_from_result(
                &question,
                None,
                policy,
                None,
                AskRetrievalContext::from(whole).with_default_gate(),
                &bridge,
                &TemporalSelection::Frontier,
            )
            .expect("about answer");
            assert_eq!(indexed, about, "{question} ({policy:?}, depth {depth})");
        }
    }
    assert!(asks >= 16, "{asks} asks answered from the index");
    assert!(pruned > 0, "the floor left nothing unread in {asks} asks");
}

#[tokio::test]
async fn maxscore_answers_as_the_whole_about_on_randomized_stores() {
    for seed in [7, 11] {
        assert_pruned_asks_equal_the_about(seed, LexicalBridge::none()).await;
    }
    let bytes = std::fs::read(shipped_bridge()).expect("the shipped table is committed");
    let bridge = LexicalBridge::from_owned_bytes(bytes).expect("table");
    assert_pruned_asks_equal_the_about(13, bridge).await;
}

/// The MCP bytes after the continuation handle, which names a read.
fn settled(value: &Value) -> String {
    let text = value.to_string();
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find("read_") {
        out.push_str(&rest[..at + 5]);
        rest = &rest[at + 5..];
        let hex = rest.chars().take_while(char::is_ascii_hexdigit).count();
        if hex == 32 {
            out.push('X');
            rest = &rest[hex..];
        }
    }
    out.push_str(rest);
    out
}

#[tokio::test]
async fn every_page_of_a_pruned_ask_is_the_whole_abouts_page() {
    let mut draw = Draw(17);
    let directory = tempfile::tempdir().expect("store");
    std::fs::write(
        directory.path().join("lexical-index.json"),
        r#"{"min_about_entries":0,"max_candidate_share_percent":100}"#,
    )
    .expect("index limits");
    let off = server(directory.path(), LexicalIndexMode::Off);
    seed(&off, &mut draw, 80).await;
    let on = server(directory.path(), LexicalIndexMode::On);
    let mut pages = 0;
    for _ in 0..4 {
        let base = json!({"about":ABOUT,"question":question(&mut draw),
            "answer_policy":"best_effort","budget":{"max_bytes":2048,"detail":"full"}});
        let mut arguments = base.clone();
        for _ in 0..40 {
            let with_index = call(&on, "kmp_ask", arguments.clone()).await;
            let without = call(&off, "kmp_ask", arguments.clone()).await;
            assert_eq!(settled(&with_index), settled(&without), "{arguments}");
            pages += 1;
            let Some(cursor) = with_index
                .pointer("/projection/page/next_cursor")
                .and_then(Value::as_str)
            else {
                break;
            };
            assert!(cursor.starts_with("kmp2:"), "{cursor}");
            arguments = base.clone();
            arguments["page"] = json!({"cursor": cursor});
        }
    }
    assert!(pages > 4, "the asks paged ({pages} pages)");
}
