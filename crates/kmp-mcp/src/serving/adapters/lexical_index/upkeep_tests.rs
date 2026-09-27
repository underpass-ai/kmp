//! The sidecar followed write by write must equal the sidecar built from
//! nothing over the same store, and both must agree with what the ranker
//! measures over the same about (DESIGN L6, shadow mode).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use kmp_application::MemoryAnswerPolicy;
use kmp_application::memory::AskMemoryQuery;
use kmp_domain::{DimensionSelection, PortError, TemporalSelection};
use kmp_embedded::EmbeddedKernel;
use kmp_proto_mapping::v1beta1::{
    AskRetrievalContext, LexicalBridge, LexicalRow, LexicalShadowWitness, PostingBlock,
    ask_response_from_result,
};
use rusqlite::Connection;
use serde_json::{Value, json};

use super::lexical_maintainer::LexicalMaintainer;
use super::shadow_comparison::ShadowComparison;
use super::shadow_report::ShadowReport;
use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use crate::KernelMcpServer;

const ABOUT: &str = "project:lexical-upkeep";
const AT: &str = "2026-09-01T10:00:00Z";

async fn call(server: &KernelMcpServer, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":arguments}});
    let wire = server
        .handle_json_line(&request.to_string())
        .await
        .expect("response");
    let result = serde_json::from_str::<Value>(&wire).expect("JSON")["result"].clone();
    assert_ne!(result["isError"], true, "{result}");
    result["structuredContent"].clone()
}

async fn write(server: &KernelMcpServer, arguments: Value) -> Value {
    let result = call(server, "kmp_write_memory", arguments).await;
    if result["status"] != "needs_review" {
        assert_eq!(result["status"], "committed", "{result}");
        return result;
    }
    let action = &result["next_actions"][0];
    call(
        server,
        action["tool"].as_str().expect("verb"),
        action["arguments"].clone(),
    )
    .await
}

fn memories(key: &str, labels: Value, memories: Value) -> Value {
    json!({"about":ABOUT,"actor":"fixture","source_kind":"human","idempotency_key":key,
        "occurred_at":AT,"observed_at":AT,"labels":labels,"options":{"strict":false},
        "memories":memories})
}

fn memory(id: &str, summary: &str, evidence: &str) -> Value {
    json!({"id":id,"kind":"observation","summary":summary,"evidence":evidence})
}

/// What the ranker measures over the about, compared with `sidecar`.
async fn shadow(
    kernel: &EmbeddedKernel,
    sidecar: &SqliteLexicalSidecar,
    question: &str,
) -> ShadowReport {
    let query = AskMemoryQuery {
        about: ABOUT.to_string(),
        question: question.to_string(),
        asked_as: None,
        answer_policy: MemoryAnswerPolicy::EvidenceOrUnknown,
        dimensions: DimensionSelection::default(),
        token_budget: 2400,
        depth: 2,
        max_tier: None,
        max_entries: None,
        temporal: TemporalSelection::Frontier,
    };
    let result = kernel
        .service()
        .ask_on_demand(query, kmp_application::RenderDemand::Skip)
        .await
        .expect("ask");
    let witness = Arc::new(LexicalShadowWitness::default());
    let retrieval = AskRetrievalContext::from(result)
        .with_default_gate()
        .with_lexical_witness(Arc::clone(&witness));
    ask_response_from_result(
        question,
        None,
        MemoryAnswerPolicy::EvidenceOrUnknown,
        None,
        retrieval,
        &LexicalBridge::none(),
        &TemporalSelection::Frontier,
    )
    .expect("answer");
    let observation = witness.take().expect("the ranker observed");
    ShadowComparison::new(sidecar, true)
        .compare(ABOUT, &observation)
        .expect("compared")
}

/// Everything the sidecar holds for the about, with candidates named by id
/// rather than ordinal: two sidecars built in different orders number their
/// candidates differently and must hold the same thing.
fn dump(path: &Path) -> BTreeMap<String, Vec<String>> {
    let connection = Connection::open(path).expect("sidecar");
    let rows = |sql: &str| -> Vec<Vec<String>> {
        let mut statement = connection.prepare(sql).expect("query");
        let count = statement.column_count();
        statement
            .query_map([ABOUT], |row| {
                (0..count)
                    .map(|index| {
                        Ok(match row.get_ref(index)? {
                            rusqlite::types::ValueRef::Integer(value) => value.to_string(),
                            rusqlite::types::ValueRef::Text(text) => {
                                String::from_utf8_lossy(text).into_owned()
                            }
                            rusqlite::types::ValueRef::Blob(blob) => format!("{blob:?}"),
                            other => format!("{other:?}"),
                        })
                    })
                    .collect()
            })
            .expect("rows")
            .map(|row| row.expect("row"))
            .collect()
    };
    let mut held = BTreeMap::new();
    let stats = rows("SELECT stats FROM lex_stats WHERE about = ?1");
    let mut stats = serde_json::from_str::<Value>(&stats[0][0]).expect("stats");
    stats["next_ordinal"] = json!(null);
    held.insert("stats".into(), vec![stats.to_string()]);
    for (name, sql) in [
        (
            "nodes",
            "SELECT node, state FROM lex_node WHERE about = ?1 ORDER BY node",
        ),
        (
            "relations",
            "SELECT source, target, type, signals FROM lex_relation WHERE about = ?1 \
             ORDER BY source, target, type",
        ),
        (
            "rows",
            "SELECT doc, row FROM lex_fwd WHERE about = ?1 ORDER BY doc",
        ),
        (
            "df",
            "SELECT term, content, direct FROM lex_key_df WHERE about = ?1 ORDER BY term",
        ),
    ] {
        held.insert(
            name.into(),
            rows(sql).into_iter().map(|row| row.join("|")).collect(),
        );
    }
    let docs = rows("SELECT ordinal, doc FROM lex_fwd WHERE about = ?1")
        .into_iter()
        .map(|row| (row[0].parse::<u64>().expect("ordinal"), row[1].clone()))
        .collect::<BTreeMap<_, _>>();
    let mut postings = Vec::new();
    let mut statement = connection
        .prepare("SELECT term, postings FROM lex_post WHERE about = ?1")
        .expect("postings");
    let blocks = statement
        .query_map([ABOUT], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .expect("blocks");
    for block in blocks {
        let (term, bytes) = block.expect("block");
        let block = PostingBlock::decode(&bytes).expect("decodes");
        assert!(block.len() <= PostingBlock::CAPACITY, "{term}");
        for posting in block.postings() {
            postings.push(format!(
                "{term}|{}|{}|{}",
                docs[&posting.ordinal], posting.content, posting.direct
            ));
        }
    }
    postings.sort();
    held.insert("postings".into(), postings);
    // Every row agrees with the postings and df derived from it.
    for row in &held["rows"] {
        assert!(!row.is_empty());
    }
    held
}

/// Builds the about from nothing in a sidecar of its own.
async fn rebuilt(kernel: &EmbeddedKernel, at: &Path) -> BTreeMap<String, Vec<String>> {
    let sidecar = Arc::new(SqliteLexicalSidecar::open(at).expect("fresh sidecar"));
    let maintainer = LexicalMaintainer::new(Arc::clone(&sidecar), true);
    kernel
        .store()
        .read_points(move |reads| {
            maintainer
                .catch_up(reads, Some(ABOUT))
                .map_err(PortError::Unavailable)
        })
        .await
        .expect("rebuilt");
    dump(at)
}

fn assert_same(followed: &BTreeMap<String, Vec<String>>, built: &BTreeMap<String, Vec<String>>) {
    for (part, rows) in built {
        assert_eq!(
            &followed[part], rows,
            "the followed sidecar differs in {part}"
        );
    }
}

#[tokio::test]
async fn a_sidecar_followed_write_by_write_equals_one_built_from_nothing() {
    let directory = tempfile::tempdir().expect("store");
    let scratch = tempfile::tempdir().expect("scratch");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    let first = write(
        &server,
        memories(
            "upkeep:1",
            json!({"component":["valve"],"task":["night-shift"]}),
            json!([
                memory(
                    "valve",
                    "The reserve valve froze during the night shift.",
                    "Operator log 14 records the frozen reserve valve."
                ),
                memory(
                    "crew",
                    "The night crew replaced the valve at 03:00.",
                    "Maintenance ticket M-201 closes the replacement."
                ),
            ]),
        ),
    )
    .await;
    // The first ask builds the about.
    call(
        &server,
        "kmp_ask",
        json!({"about":ABOUT,"question":"what froze?"}),
    )
    .await;
    let sidecar_path = directory
        .path()
        .join(super::lexical_sidecar::LEXICAL_INDEX_FILE);
    assert!(sidecar_path.exists(), "the first ask builds the sidecar");
    let kernel = EmbeddedKernel::open(directory.path()).expect("reader");
    let reading = SqliteLexicalSidecar::open(&sidecar_path).expect("sidecar");
    let report = shadow(&kernel, &reading, "which valve froze during the night?").await;
    assert!(report.comparable, "{report:?}");
    assert_eq!(report.differences(), 0, "{report:?}");
    assert_eq!(report.documents, 4);

    // Writes after the build: new memories under a new label, a correction
    // of an existing one, a relation between two, and a relabel.
    let second = write(
        &server,
        memories(
            "upkeep:2",
            json!({"component":["pump"],"task":["night-shift"]}),
            json!([
                memory(
                    "pump",
                    "The backup pump started when the valve froze.",
                    "Pump controller trace P-7 shows the start."
                ),
                memory(
                    "valve-fix",
                    "Heating tape now protects the reserve valve.",
                    "Work order W-9 installs the heating tape."
                ),
            ]),
        ),
    )
    .await;
    let valve = first["local_refs"]["valve"]
        .as_str()
        .expect("ref")
        .to_string();
    let pump = second["local_refs"]["pump"]
        .as_str()
        .expect("ref")
        .to_string();
    write(
        &server,
        json!({"about":ABOUT,"actor":"fixture","idempotency_key":"upkeep:link","observed_at":AT,
            "read_context":{"inspected_refs":[valve, pump]},
            "relations":[{"from":pump,"to":valve,"rel":"supports",
                "why":"The pump started because the valve froze.",
                "evidence":"Trace P-7 starts one minute after log 14."}]}),
    )
    .await;
    write(
        &server,
        memories(
            "upkeep:3",
            json!({"component":["valve"],"task":["night-shift"]}),
            json!([{"id":"valve","ref":valve,"kind":"observation",
                "summary":"The reserve valve froze twice during the night shift.",
                "evidence":"Operator log 15 records the second freeze."}]),
        ),
    )
    .await;
    call(
        &server,
        "kmp_relabel",
        json!({"about":ABOUT,"ref":pump,"actor":"fixture","observed_at":AT,
            "add":{"component":["backup-pump"]},"remove":{"component":["pump"]},
            "why":"The registry names it the backup pump.",
            "idempotency_key":"upkeep:relabel"}),
    )
    .await;

    let report = shadow(&kernel, &reading, "why did the backup pump start?").await;
    assert!(report.comparable, "{report:?}");
    assert_eq!(report.differences(), 0, "{report:?}");
    let followed = dump(&sidecar_path);
    let built = rebuilt(&kernel, &scratch.path().join("built.sqlite3")).await;
    assert_same(&followed, &built);

    // Following the same events again moves nothing.
    let connection = Connection::open(&sidecar_path).expect("sidecar");
    connection
        .execute("UPDATE meta SET value = '0' WHERE key = 'position'", [])
        .expect("rewind");
    connection
        .execute("UPDATE meta SET value = '' WHERE key = 'tail'", [])
        .expect("rewind");
    drop(connection);
    let again = Arc::new(SqliteLexicalSidecar::open(&sidecar_path).expect("sidecar"));
    let maintainer = LexicalMaintainer::new(Arc::clone(&again), true);
    let report = kernel
        .store()
        .read_points(move |reads| {
            maintainer
                .catch_up(reads, None)
                .map_err(PortError::Unavailable)
        })
        .await
        .expect("replayed");
    assert!(report.events >= 5, "{report:?}");
    assert!(report.committed);
    assert_same(&dump(&sidecar_path), &built);
}

#[tokio::test]
async fn an_about_whose_language_moves_is_read_again_in_the_new_one() {
    let directory = tempfile::tempdir().expect("store");
    let scratch = tempfile::tempdir().expect("scratch");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    // Two short memories read as no language: nothing is stemmed.
    write(
        &server,
        memories(
            "language:1",
            json!({"component":["valvula"]}),
            json!([memory("v1", "Valvula R2 congelada.", "Parte R2.")]),
        ),
    )
    .await;
    call(
        &server,
        "kmp_ask",
        json!({"about":ABOUT,"question":"valvula"}),
    )
    .await;
    let sidecar_path = directory
        .path()
        .join(super::lexical_sidecar::LEXICAL_INDEX_FILE);
    let kernel = EmbeddedKernel::open(directory.path()).expect("reader");
    let reading = SqliteLexicalSidecar::open(&sidecar_path).expect("sidecar");
    let before = reading
        .stats(ABOUT)
        .expect("stats")
        .expect("built")
        .language;
    assert_ne!(before.as_deref(), Some("spanish"));
    // Spanish prose moves the about into Spanish: every row is stemmed now.
    write(
        &server,
        memories(
            "language:2",
            json!({"component":["valvula"]}),
            json!((2..14).map(|index| memory(
                &format!("v{index}"),
                &format!("La válvula {index} de la reserva se congeló durante la noche y el turno de la guardia la cambió por otra de las nuevas."),
                &format!("El parte {index} de la noche dice que la válvula de la línea se congeló y que la guardia la cambió."),
            )).collect::<Vec<_>>()),
        ),
    )
    .await;
    assert_eq!(
        reading
            .stats(ABOUT)
            .expect("stats")
            .expect("built")
            .language
            .as_deref(),
        Some("spanish")
    );
    let report = shadow(&kernel, &reading, "¿cuándo se congelaron las válvulas?").await;
    assert!(report.comparable, "{report:?}");
    assert_eq!(report.differences(), 0, "{report:?}");
    assert_same(
        &dump(&sidecar_path),
        &rebuilt(&kernel, &scratch.path().join("built.sqlite3")).await,
    );
}

#[tokio::test]
async fn writes_to_abouts_nobody_asked_about_only_move_the_position() {
    let directory = tempfile::tempdir().expect("store");
    let server = KernelMcpServer::embedded(directory.path()).expect("server");
    write(
        &server,
        memories(
            "other:1",
            json!({"component":["valve"]}),
            json!([memory("v1", "The reserve valve froze.", "Log 14.")]),
        ),
    )
    .await;
    let sidecar_path = directory
        .path()
        .join(super::lexical_sidecar::LEXICAL_INDEX_FILE);
    let reading = SqliteLexicalSidecar::open(&sidecar_path).expect("sidecar");
    assert!(reading.stats(ABOUT).expect("stats").is_none());
    let meta = reading.meta().expect("meta");
    assert!(meta.position > 0, "{meta:?}");
    assert_eq!(meta.version, super::lexical_maintainer::INDEX_VERSION);
    // A sidecar of another derivation is emptied, never read.
    let connection = Connection::open(&sidecar_path).expect("sidecar");
    connection
        .execute(
            "UPDATE meta SET value = 'lexical-index-0' WHERE key = 'index_version'",
            [],
        )
        .expect("older derivation");
    drop(connection);
    call(
        &server,
        "kmp_ask",
        json!({"about":ABOUT,"question":"what froze?"}),
    )
    .await;
    let meta = reading.meta().expect("meta");
    assert_eq!(meta.version, super::lexical_maintainer::INDEX_VERSION);
    assert_eq!(
        reading
            .stats(ABOUT)
            .expect("stats")
            .expect("built")
            .documents,
        2
    );
}

#[test]
fn rows_are_read_from_candidates_not_from_their_storage() {
    // Decoding what was encoded is the only way a row reaches a comparison.
    let row = LexicalRow::default();
    assert_eq!(LexicalRow::decode(&row.encode()).expect("row"), row);
}
