use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::json;

use super::cassette_judgement::{CassetteJudgement, judgement_key};
use super::observed_judgement::ObservedJudgement;
use super::sqlite_verdict_book::SqliteVerdictBook;
use super::verdict_ledger::VerdictLedger;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::judgement_site::JudgementSite;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::telemetry::captured_log::CapturedLog;

const DEBUG: &str = "kmp_mcp=info,kmp_mcp::judgement=debug";

/// Answers every noul with 0.7 over two provider requests.
struct TwoRequests;

impl JudgementModel for TwoRequests {
    fn model(&self) -> &str {
        "jev-1.13.0"
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>,
    > {
        let answers = request
            .questions
            .keys()
            .map(|key| (key.clone(), JudgementAnswer::Noul { yes: 0.7 }))
            .collect::<BTreeMap<_, _>>();
        Box::pin(async move {
            Ok(JudgementResponse {
                model: "jev-1.13.0".into(),
                answers,
                input_tokens: 321,
                requests: 2,
                elapsed_us: 0,
            })
        })
    }
}

fn request(text: &str, questions: usize) -> JudgementRequest {
    JudgementRequest {
        state: json!(text),
        questions: (0..questions)
            .map(|n| {
                (
                    format!("c{n}"),
                    JudgementQuestion::Noul {
                        instructions: json!("Does it answer?"),
                    },
                )
            })
            .collect(),
    }
}

#[tokio::test]
async fn each_evaluation_leaves_one_line_with_site_counts_tokens_and_time() {
    let (log, _guard) = CapturedLog::start(DEBUG);
    let observed = ObservedJudgement::new(Arc::new(TwoRequests), JudgementSite::Rerank);
    let asked = request("state", 3);

    let response = observed.evaluate(&asked).await.expect("answered");

    assert_eq!(response.input_tokens, 321, "answers pass through");
    let lines = log.events("kmp_judgement");
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["target"], "kmp_mcp::judgement");
    assert_eq!(line["level"], "DEBUG");
    let fields = &line["fields"];
    assert_eq!(fields["site"], "rerank");
    assert_eq!(fields["model"], "jev-1.13.0");
    assert_eq!(fields["source"], "remote");
    assert_eq!(fields["status"], "ok");
    assert_eq!(fields["questions"], 3);
    assert_eq!(fields["answers"], 3);
    assert_eq!(fields["requests"], 2);
    assert_eq!(fields["http_requests"], 2);
    assert_eq!(fields["input_tokens"], 321);
    assert!(fields["elapsed_us"].is_u64());
    assert_eq!(
        fields["request_key"],
        judgement_key("jev-1.13.0", &asked).as_str()
    );
    assert!(
        !line.to_string().contains("Does it answer"),
        "no text leaks"
    );
}

#[tokio::test]
async fn a_cassette_says_whether_it_hit_or_missed() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("jev.cassette.json");
    CassetteJudgement::record(&path, Arc::new(TwoRequests))
        .expect("record")
        .evaluate(&request("kept", 1))
        .await
        .expect("recorded");
    let replay: Arc<dyn JudgementModel> =
        Arc::new(CassetteJudgement::replay(&path, "jev-1.13.0".into()).expect("replay"));
    let (log, _guard) = CapturedLog::start(DEBUG);
    let observed = ObservedJudgement::new(replay, JudgementSite::Paths);

    observed.evaluate(&request("kept", 1)).await.expect("hit");
    let missing = observed
        .evaluate(&request("never recorded", 1))
        .await
        .expect_err("miss");

    let lines = log.events("kmp_judgement");
    assert_eq!(lines.len(), 2);
    let hit = &lines[0]["fields"];
    assert_eq!(hit["site"], "paths");
    assert_eq!(hit["source"], "cassette_hit");
    assert_eq!(hit["requests"], 2, "what the recording cost");
    assert_eq!(hit["http_requests"], 0, "what this process sent");
    assert_eq!(hit["input_tokens"], 321);
    let miss = &lines[1]["fields"];
    assert_eq!(miss["source"], "cassette_miss");
    assert_eq!(miss["status"], "error");
    assert!(miss.get("input_tokens").is_none(), "no tokens, never zero");
    assert_eq!(miss["error_hash"].as_str().map(str::len), Some(16));
    assert!(!lines[1].to_string().contains(&missing), "no message leaks");
}

#[tokio::test]
async fn a_record_miss_is_reported_as_a_miss_that_reached_the_model() {
    let dir = tempfile::tempdir().expect("dir");
    let recorder: Arc<dyn JudgementModel> = Arc::new(
        CassetteJudgement::record(&dir.path().join("jev.cassette.json"), Arc::new(TwoRequests))
            .expect("record"),
    );
    let (log, _guard) = CapturedLog::start(DEBUG);
    let observed = ObservedJudgement::new(recorder, JudgementSite::Labels);

    observed.evaluate(&request("new", 2)).await.expect("asked");
    observed.evaluate(&request("new", 2)).await.expect("kept");

    let origins = log
        .events("kmp_judgement")
        .iter()
        .map(|line| {
            (
                line["fields"]["source"].as_str().map(str::to_string),
                line["fields"]["http_requests"].as_u64(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        origins,
        vec![
            (Some("cassette_miss".into()), Some(2)),
            (Some("cassette_hit".into()), Some(0)),
        ]
    );
}

#[tokio::test]
async fn nothing_is_written_or_hashed_below_debug() {
    let (log, _guard) = CapturedLog::start("kmp_mcp=info");
    let observed = ObservedJudgement::new(Arc::new(TwoRequests), JudgementSite::WakeFocus);

    observed.evaluate(&request("s", 1)).await.expect("answered");

    assert!(log.events("kmp_judgement").is_empty());
    assert_eq!(observed.model(), "jev-1.13.0");
}

#[test]
fn a_site_view_keeps_absence_and_the_reason_a_store_cannot_judge() {
    let off: Result<Option<Arc<dyn JudgementModel>>, String> = Ok(None);
    assert!(matches!(
        ObservedJudgement::for_site(&off, None, JudgementSite::Rerank),
        Ok(None)
    ));
    let broken: Result<Option<Arc<dyn JudgementModel>>, String> = Err("no key".into());
    assert_eq!(
        ObservedJudgement::for_site(&broken, None, JudgementSite::Rerank).err(),
        Some("no key".to_string())
    );
    let on: Result<Option<Arc<dyn JudgementModel>>, String> = Ok(Some(Arc::new(TwoRequests)));
    let observed = ObservedJudgement::for_site(&on, None, JudgementSite::Summaries)
        .expect("on")
        .expect("model");
    assert_eq!(observed.model(), "jev-1.13.0");
}

#[tokio::test]
async fn a_book_hit_says_so_sends_nothing_and_keeps_the_request_key() {
    let dir = tempfile::tempdir().expect("dir");
    let cassette = dir.path().join("jev.cassette.json");
    CassetteJudgement::record(&cassette, Arc::new(TwoRequests))
        .expect("record")
        .evaluate(&request("kept", 2))
        .await
        .expect("recorded");
    let replay: Arc<dyn JudgementModel> =
        Arc::new(CassetteJudgement::replay(&cassette, "jev-1.13.0".into()).expect("replay"));
    let book =
        SqliteVerdictBook::open(&dir.path().join("judgements.sqlite3"), 1 << 20).expect("book");
    let ledger = Arc::new(VerdictLedger::new(Arc::new(book)));
    let on: Result<Option<Arc<dyn JudgementModel>>, String> = Ok(Some(replay));
    let observed = ObservedJudgement::for_site(&on, Some(&ledger), JudgementSite::Rerank)
        .expect("on")
        .expect("model");
    let (log, _guard) = CapturedLog::start(DEBUG);

    let first = observed
        .evaluate(&request("kept", 2))
        .await
        .expect("cassette");
    let again = observed.evaluate(&request("kept", 2)).await.expect("book");

    assert_eq!(first.answers, again.answers);
    assert_eq!((again.requests, again.input_tokens), (0, 0));
    let lines = log.events("kmp_judgement");
    let fields = lines
        .iter()
        .map(|line| {
            (
                line["fields"]["source"].as_str().map(str::to_string),
                line["fields"]["http_requests"].as_u64(),
                line["fields"]["request_key"].as_str().map(str::to_string),
            )
        })
        .collect::<Vec<_>>();
    let key = Some(judgement_key("jev-1.13.0", &request("kept", 2)));
    assert_eq!(
        fields,
        vec![
            (Some("cassette_hit".into()), Some(0), key.clone()),
            (Some("book_hit".into()), Some(0), key),
        ]
    );
}
