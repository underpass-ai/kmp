//! An ask answered from the lexical index (DESIGN L6, P13) gives the bytes
//! the same ask gives reading the whole about: the candidates the postings
//! of the question's words, their associations and the words the bridge
//! finds reach, ranked against the whole about's statistics, lifecycle and
//! vocabulary. Asks the index cannot hold read the about, declared.

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

/// Unrelated notes, so the questions reach a small part of the about.
const FILLERS: usize = 160;
const EARLY: &str = "2026-09-01T10:00:00Z";
const LATE: &str = "2026-09-01T12:00:00Z";

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

/// Unrelated notes, so the question's words reach a small part of
/// the about, and a few memories the questions are about: a support, an
/// expiry, a replacement, and a Spanish note the table bridges to.
async fn seed(server: &KernelMcpServer) {
    let fillers = (0..FILLERS)
        .map(|index| {
            json!({"id":format!("note-{index}"),"kind":"observation",
                "summary":format!("Garden bed {index} received compost batch {} and mulch.", index % 7),
                "evidence":format!("Gardening diary page {index} lists the compost.")})
        })
        .collect::<Vec<_>>();
    write(server, packet("seed:fillers", EARLY, json!(fillers))).await;
    let first = write(
        server,
        packet(
            "seed:valve",
            EARLY,
            json!([
                {"id":"valve","kind":"observation",
                 "summary":"The reserve valve froze during the night shift.",
                 "evidence":"Operator log 14 records the frozen reserve valve."},
                {"id":"crew","kind":"observation",
                 "summary":"The night crew replaced the valve seal at 03:00.",
                 "evidence":"Maintenance ticket M-201 closes the replacement."},
                {"id":"tape","kind":"decision",
                 "summary":"Heating tape protects the reserve valve from frost.",
                 "evidence":"Work order W-9 installs the heating tape.",
                 "valid_from":EARLY,"valid_until":"2026-09-01T11:00:00Z"},
                {"id":"nota","kind":"observation",
                 "summary":"La válvula de reserva se congeló durante la noche.",
                 "evidence":"El parte de turno lo anota."}
            ]),
        ),
    )
    .await;
    let refs = &first["local_refs"];
    write(
        server,
        packet(
            "seed:after",
            LATE,
            json!([
                {"id":"pump","kind":"observation",
                 "summary":"The backup pump started when the valve froze.",
                 "evidence":"Pump controller trace P-7 shows the start.",
                 "connect_to":[{"ref":refs["valve"],"rel":"supports","class":"evidential",
                    "why":"The pump started because the valve froze.",
                    "evidence":"Trace P-7 starts one minute after log 14.","confidence":"high"}]},
                {"id":"glycol","kind":"decision",
                 "summary":"Glycol heating replaces the heating tape on the reserve valve.",
                 "evidence":"Work order W-12 removes the tape.",
                 "connect_to":[{"ref":refs["tape"],"rel":"supersedes","class":"evidential",
                    "why":"Glycol heating replaces the tape.",
                    "evidence":"W-12 names W-9 as replaced.","confidence":"high"}]}
            ]),
        ),
    )
    .await;
}

const QUESTIONS: [&str; 7] = [
    "which valve froze during the night?",
    "why did the backup pump start?",
    "what protects the reserve valve from frost?",
    "who replaced the valve seal?",
    "what does operator log 14 record?",
    "¿qué válvula se congeló de noche?",
    "what replaced the heating tape?",
];

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

/// Every question answered from the index and from the about, under both
/// policies (with and without the anchored gate's alias terms): the same
/// response, field for field.
async fn assert_index_answers_as_the_about(bridge: LexicalBridge, bridged: bool) {
    let directory = tempfile::tempdir().expect("store");
    seed(&server(directory.path(), LexicalIndexMode::Off)).await;
    let kernel = EmbeddedKernel::open(directory.path()).expect("kernel");
    let sidecar = LexicalSidecar::open(
        directory.path(),
        LexicalIndexMode::On,
        IndexLimits::DEFAULT.every_about(),
    );
    let service = kernel.service();
    let mut reached_by_the_bridge = false;
    let mut answered = 0;
    for question in QUESTIONS {
        for (policy, depth) in [
            (MemoryAnswerPolicy::EvidenceOrUnknown, 2),
            (MemoryAnswerPolicy::BestEffort, 2),
            // Deeper, while nothing lies past the indexed depth.
            (MemoryAnswerPolicy::EvidenceOrUnknown, 3),
        ] {
            let query = query(question, policy, depth);
            let followed = sidecar.catch_up(kernel.store(), Some(ABOUT)).await;
            let answer = |read: &super::indexed_read::IndexedRead| {
                ask_response_from_result(
                    question,
                    None,
                    policy,
                    None,
                    AskRetrievalContext::from(read.result.clone())
                        .with_default_gate()
                        .with_indexed(read.indexed.clone()),
                    &bridge,
                    &TemporalSelection::Frontier,
                )
                .map_err(|status| status.message().to_string())
            };
            let (read, indexed) = match sidecar
                .indexed_read(
                    kernel.store(),
                    &service,
                    &query,
                    followed.as_ref(),
                    &bridge,
                    depth > 2,
                    None,
                    0,
                    &answer,
                )
                .await
                .expect("the index reads")
            {
                Ok(read) => read,
                // A word that reaches most of the about, or associations
                // or bridged words that do, send the ask to the about.
                Err(why) => {
                    assert_eq!(why, "the candidates cover too much of the about");
                    continue;
                }
            };
            answered += 1;
            assert!(
                read.candidates as u64 * 20 <= read.documents * 7,
                "{question}: {} of {}",
                read.candidates,
                read.documents
            );
            reached_by_the_bridge |= read.indexed.vocabulary.is_some();
            let whole = service
                .ask_on_demand(query, kmp_application::RenderDemand::Skip)
                .await
                .expect("ask");
            let about = ask_response_from_result(
                question,
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
    assert_eq!(reached_by_the_bridge, bridged);
    assert!(
        answered >= QUESTIONS.len() * 3 / 2,
        "{answered} answered from the index"
    );
}

#[tokio::test]
async fn an_ask_answered_from_the_postings_equals_the_whole_about() {
    assert_index_answers_as_the_about(LexicalBridge::none(), false).await;
}

#[tokio::test]
async fn the_bridge_reads_the_whole_abouts_vocabulary_from_the_index() {
    let bytes = std::fs::read(shipped_bridge()).expect("the shipped table is committed");
    let bridge = LexicalBridge::from_owned_bytes(bytes).expect("table");
    assert_index_answers_as_the_about(bridge, true).await;
}

#[tokio::test]
async fn a_question_that_reaches_most_of_the_about_reads_the_about() {
    let directory = tempfile::tempdir().expect("store");
    seed(&server(directory.path(), LexicalIndexMode::Off)).await;
    let kernel = EmbeddedKernel::open(directory.path()).expect("kernel");
    let sidecar = LexicalSidecar::open(
        directory.path(),
        LexicalIndexMode::On,
        IndexLimits::DEFAULT.every_about(),
    );
    let query = query(
        "which garden bed received compost and mulch?",
        MemoryAnswerPolicy::BestEffort,
        2,
    );
    let followed = sidecar.catch_up(kernel.store(), Some(ABOUT)).await;
    let read = sidecar
        .indexed_read(
            kernel.store(),
            &kernel.service(),
            &query,
            followed.as_ref(),
            &LexicalBridge::none(),
            false,
            None,
            0,
            &|_| Ok(Default::default()),
        )
        .await
        .expect("the index reads");
    assert_eq!(
        read.err(),
        Some("the candidates cover too much of the about")
    );
    // Without following the log first the index does not answer.
    let read = sidecar
        .indexed_read(
            kernel.store(),
            &kernel.service(),
            &query,
            None,
            &LexicalBridge::none(),
            false,
            None,
            0,
            &|_| Ok(Default::default()),
        )
        .await
        .expect("the index reads");
    assert_eq!(read.err(), Some("the index did not follow the log"));
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
async fn every_ask_gives_the_same_bytes_with_the_index_on() {
    let directory = tempfile::tempdir().expect("store");
    std::fs::copy(
        shipped_bridge(),
        directory.path().join("lexical-bridge.kmpb"),
    )
    .expect("store table");
    // The about is small: index it all the same, to answer from the index.
    std::fs::write(
        directory.path().join("lexical-index.json"),
        r#"{"min_about_entries":0}"#,
    )
    .expect("index limits");
    let off = server(directory.path(), LexicalIndexMode::Off);
    seed(&off).await;
    let on = server(directory.path(), LexicalIndexMode::On);
    let mut asks = QUESTIONS
        .iter()
        .map(|question| json!({"about":ABOUT,"question":question}))
        .collect::<Vec<_>>();
    // A deeper ask (held while nothing lies past the indexed depth), and
    // asks the index does not hold: a shallower one, a clock, a dimension,
    // a question that reaches most of the about.
    asks.extend([
        json!({"about":ABOUT,"question":QUESTIONS[0],"depth":1}),
        json!({"about":ABOUT,"question":QUESTIONS[2],"depth":3}),
        json!({"about":ABOUT,"question":QUESTIONS[0],"as_of":{"time":EARLY}}),
        json!({"about":ABOUT,"question":QUESTIONS[1],
            "dimensions":{"scope":"current_about","selectors":[{"key":"component","op":"in","values":["plant"]}]}}),
        json!({"about":ABOUT,"question":"garden bed compost mulch","answer_policy":"best_effort"}),
    ]);
    for ask in asks {
        let with_index = call(&on, "kmp_ask", ask.clone()).await;
        let without = call(&off, "kmp_ask", ask.clone()).await;
        assert_eq!(settled(&with_index), settled(&without), "{ask}");
    }
    // A write the index follows is answered from it at once.
    write(
        &on,
        packet(
            "seed:later",
            LATE,
            json!([{"id":"thaw","kind":"observation",
                "summary":"The reserve valve thawed at dawn.",
                "evidence":"Operator log 15 records the thaw."}]),
        ),
    )
    .await;
    let ask = json!({"about":ABOUT,"question":"when did the reserve valve thaw?"});
    assert_eq!(
        settled(&call(&on, "kmp_ask", ask.clone()).await),
        settled(&call(&off, "kmp_ask", ask).await)
    );
}

/// Whether the sidecar beside `path` holds `ABOUT`.
fn indexed(path: &Path) -> bool {
    super::sqlite_lexical_sidecar::SqliteLexicalSidecar::open(
        &path.join(super::lexical_sidecar::LEXICAL_INDEX_FILE),
    )
    .expect("sidecar")
    .stats(ABOUT)
    .expect("stats")
    .is_some()
}

#[tokio::test]
async fn an_about_below_the_size_threshold_is_read_and_never_indexed() {
    let directory = tempfile::tempdir().expect("store");
    let off = server(directory.path(), LexicalIndexMode::Off);
    seed(&off).await;
    // 166 entries, below the default threshold.
    let on = server(directory.path(), LexicalIndexMode::On);
    let ask = json!({"about":ABOUT,"question":QUESTIONS[0]});
    assert_eq!(
        settled(&call(&on, "kmp_ask", ask.clone()).await),
        settled(&call(&off, "kmp_ask", ask.clone()).await)
    );
    assert!(!indexed(directory.path()), "a small about is not built");
    // A store that lowers the threshold indexes it on its next ask.
    std::fs::write(
        directory.path().join("lexical-index.json"),
        r#"{"min_about_entries":100}"#,
    )
    .expect("index limits");
    let lowered = server(directory.path(), LexicalIndexMode::On);
    assert_eq!(
        settled(&call(&lowered, "kmp_ask", ask.clone()).await),
        settled(&call(&off, "kmp_ask", ask).await)
    );
    assert!(indexed(directory.path()));
}
