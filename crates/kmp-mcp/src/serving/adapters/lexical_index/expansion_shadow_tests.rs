//! A store whose memories carry judged search expansions (P15, field X): the
//! sidecar reads them as the ranker does, apart from both fields, its
//! postings reach a memory only its expansions name, and the shadow still
//! finds no difference, built from nothing or followed write by write.

use kmp_application::MemoryAnswerPolicy;
use kmp_embedded::EmbeddedKernel;
use serde_json::{Value, json};

use super::sqlite_lexical_sidecar::SqliteLexicalSidecar;
use super::upkeep_tests::{
    ABOUT, assert_same, call, dump, memories, rebuilt, shadow, shadow_under, write,
};
use crate::serving::lexical_index_mode::LexicalIndexMode;
use crate::serving::ports::lexical_candidates::LexicalCandidates;
use crate::{EmbeddedKernelMcpBackend, KernelMcpServer, KernelMcpToolBackend, KernelMcpToolFuture};

/// The embedded store in shadow with a judge that accepts every expansion
/// naming `launch` (0.9) and refuses every other (0.1).
struct JudgingBackend {
    inner: EmbeddedKernelMcpBackend,
}

impl KernelMcpToolBackend for JudgingBackend {
    fn backend_name(&self) -> &'static str {
        "embedded"
    }

    fn call_tool<'a>(&'a self, name: &'a str, arguments: &'a Value) -> KernelMcpToolFuture<'a> {
        if name == "kmp_curate" && arguments["mode"] == "judge_expansions" {
            let verdicts = arguments["items"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|item| {
                    let yes = item["expansions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|expansion| {
                            let text = expansion.as_str().unwrap_or_default().to_lowercase();
                            if text.contains("launch") { 0.9 } else { 0.1 }
                        })
                        .collect::<Vec<_>>();
                    json!({"ref": item["ref"], "yes": yes})
                })
                .collect::<Vec<_>>();
            let answer = json!({"structuredContent": {
                "enabled": true, "accept_at": 0.5, "judged_by": "jev-test noul>=0.50",
                "verdicts": verdicts, "jev": {"requests": 1, "input_tokens": 120}
            }});
            return Box::pin(async move { Ok(answer) });
        }
        self.inner.call_tool(name, arguments)
    }
}

fn expanded(id: &str, summary: &str, evidence: &str, expansions: &[&str]) -> Value {
    json!({"id":id,"kind":"decision","summary":summary,"evidence":evidence,
        "search_expansions":expansions})
}

#[tokio::test]
async fn expansions_are_indexed_as_the_ranker_reads_them_with_no_shadow_difference() {
    let directory = tempfile::tempdir().expect("store");
    let scratch = tempfile::tempdir().expect("scratch");
    let server = KernelMcpServer::with_backend(JudgingBackend {
        inner: EmbeddedKernelMcpBackend::open(directory.path())
            .expect("store")
            .with_lexical_index(LexicalIndexMode::Shadow),
    });
    let first = write(
        &server,
        memories(
            "expansions:1",
            json!({"work":["main"]}),
            json!([
                expanded(
                    "rollout",
                    "The rollout slipped because the auditors had not signed off.",
                    "Release notes 12 record the slip.",
                    &["Why was the launch postponed?"]
                ),
                expanded(
                    "canteen",
                    "The canteen menu changed on Tuesday for the whole building.",
                    "The notice board lists the new menu.",
                    &[]
                ),
            ]),
        ),
    )
    .await;
    assert_eq!(
        first["search_expansions"]["stored"]
            .as_object()
            .map(|stored| stored.len()),
        Some(1),
        "{first}"
    );
    // The first ask builds the about, expansions included.
    let answer = call(
        &server,
        "kmp_ask",
        json!({"about":ABOUT,"question":"Why was the launch postponed?",
            "answer_policy":"best_effort"}),
    )
    .await;
    assert!(
        answer.to_string().contains("\"reached_by\":\"expansion\""),
        "{answer}"
    );
    // A second expanded memory, followed from the log after the build.
    write(
        &server,
        memories(
            "expansions:2",
            json!({"work":["main"]}),
            json!([expanded(
                "freeze",
                "Deployments stay frozen until the security review closes.",
                "Change board minutes 7.",
                &[
                    "When can we launch again?",
                    "a question nobody judged relevant"
                ]
            )]),
        ),
    )
    .await;
    call(
        &server,
        "kmp_ask",
        json!({"about":ABOUT,"question":"what changed in the canteen?"}),
    )
    .await;

    let sidecar_path = directory
        .path()
        .join(super::lexical_sidecar::LEXICAL_INDEX_FILE);
    let kernel = EmbeddedKernel::open(directory.path()).expect("reader");
    let reading = SqliteLexicalSidecar::open(&sidecar_path).expect("sidecar");
    let stats = reading.stats(ABOUT).expect("stats").expect("built");
    assert_eq!(stats.expanded, 2, "{stats:?}");
    assert!(stats.expansion_length > 0);
    // `launch` is in no memory's words: only the postings of the expansions
    // reach the two memories, with the X counts the rescue scores.
    let launch = reading
        .candidates(ABOUT, &["launch".to_string()])
        .expect("candidates");
    assert_eq!(launch.len(), 2, "{launch:?}");
    for row in launch.values() {
        let term = row.term("launch").expect("held");
        assert_eq!((term.content(true), term.direct(true)), (0, 0));
        assert_eq!(term.expansion, 1);
    }
    for question in [
        "Why was the launch postponed?",
        "When can we launch again?",
        "why did the rollout slip?",
    ] {
        let report = shadow(&kernel, &reading, question).await;
        assert!(report.comparable, "{report:?}");
        assert_eq!(report.differences(), 0, "{question}: {report:?}");
        let plain = shadow_under(&kernel, &reading, question, MemoryAnswerPolicy::BestEffort).await;
        assert_eq!(plain.differences(), 0, "{question}: {plain:?}");
    }
    let followed = dump(&sidecar_path);
    let built = rebuilt(&kernel, &scratch.path().join("built.sqlite3")).await;
    assert_same(&followed, &built);
}
