use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::serving::adapters::sqlite_verdict_book::SqliteVerdictBook;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::ports::verdict_book::VerdictBook;

/// Answers each noul with `yes` plus a per-question jitter a real model
/// would show between samples, and each choice with its last option; keeps
/// every question name it was asked.
struct Jittery {
    yes: f64,
    delay: Duration,
    asked: Mutex<Vec<Vec<String>>>,
}

impl Jittery {
    fn new(yes: f64) -> Arc<Self> {
        Arc::new(Self {
            yes,
            delay: Duration::ZERO,
            asked: Mutex::new(Vec::new()),
        })
    }

    fn asked(&self) -> Vec<Vec<String>> {
        self.asked.lock().expect("asked").clone()
    }
}

impl JudgementModel for Jittery {
    fn model(&self) -> &str {
        "jev-test"
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>>
    {
        Box::pin(async move {
            tokio::time::sleep(self.delay).await;
            self.asked
                .lock()
                .expect("asked")
                .push(request.questions.keys().cloned().collect());
            let answers = request
                .questions
                .iter()
                .enumerate()
                .map(|(n, (name, question))| {
                    let answer = match question {
                        JudgementQuestion::Noul { .. } => JudgementAnswer::Noul {
                            yes: self.yes + n as f64 * 0.0123457,
                        },
                        JudgementQuestion::Choice { options, .. } => JudgementAnswer::Choice {
                            choice: options.last().expect("option").clone(),
                            probabilities: options
                                .iter()
                                .map(|option| (option.clone(), 1.0 / options.len() as f64))
                                .collect(),
                            confidence: 0.66666,
                        },
                        JudgementQuestion::Score { levels, .. } => JudgementAnswer::Score {
                            score: 1.0,
                            probabilities: levels
                                .iter()
                                .map(|_| 1.0 / levels.len() as f64)
                                .collect(),
                            confidence: 0.5,
                        },
                    };
                    (name.clone(), answer)
                })
                .collect::<BTreeMap<_, _>>();
            Ok(JudgementResponse {
                model: "jev-test".into(),
                answers,
                input_tokens: 90,
                requests: 1,
                elapsed_us: 0,
            })
        })
    }
}

fn request(passages: &[&str]) -> JudgementRequest {
    JudgementRequest {
        state: json!("Which gateway fronts the platform?"),
        questions: passages
            .iter()
            .enumerate()
            .map(|(n, passage)| {
                (
                    format!("p{n}"),
                    JudgementQuestion::Noul {
                        instructions: json!({"passage": passage, "question": "q"}),
                    },
                )
            })
            .collect(),
    }
}

fn ledger(dir: &tempfile::TempDir) -> Arc<VerdictLedger> {
    let book =
        SqliteVerdictBook::open(&dir.path().join("judgements.sqlite3"), 1 << 20).expect("book");
    Arc::new(VerdictLedger::new(Arc::new(book)))
}

fn ledgered(model: &Arc<Jittery>, ledger: &Arc<VerdictLedger>) -> LedgeredJudgement<Arc<Jittery>> {
    LedgeredJudgement::new(Arc::clone(model), Arc::clone(ledger), JudgementSite::Rerank)
}

#[tokio::test]
async fn a_repeated_judgement_is_answered_by_the_book_with_the_same_bits() {
    let dir = tempfile::tempdir().expect("dir");
    let model = Jittery::new(0.61);
    let judged = ledgered(&model, &ledger(&dir));
    let first_request = request(&["a", "b", "c"]);
    let (first, origin) = judged.evaluate_traced(&first_request).await;
    let first = first.expect("first");
    assert_eq!(
        (origin, first.requests, first.input_tokens),
        (JudgementOrigin::Remote, 1, 90)
    );
    let (again, origin) = judged.evaluate_traced(&first_request).await;
    let again = again.expect("again");
    assert_eq!(origin, JudgementOrigin::BookHit);
    assert_eq!((again.requests, again.input_tokens), (0, 0));
    assert_eq!(
        again.answers, first.answers,
        "the first call already read quantized answers"
    );
    assert_eq!(
        serde_json::to_string(&again.answers).expect("json"),
        serde_json::to_string(&first.answers).expect("json")
    );
    assert_eq!(model.asked().len(), 1);
}

#[tokio::test]
async fn only_the_questions_the_book_lacks_reach_the_model() {
    let dir = tempfile::tempdir().expect("dir");
    let model = Jittery::new(0.3);
    let judged = ledgered(&model, &ledger(&dir));
    judged.evaluate(&request(&["a", "b"])).await.expect("first");
    let (both, origin) = judged.evaluate_traced(&request(&["b", "a", "new"])).await;
    let both = both.expect("second");
    assert_eq!(origin, JudgementOrigin::Remote);
    assert_eq!(
        model.asked(),
        vec![vec!["p0".to_string(), "p1".into()], vec!["p2".into()]]
    );
    assert_eq!(both.answers.len(), 3);
}

#[tokio::test]
async fn another_process_that_judged_first_wins() {
    let dir = tempfile::tempdir().expect("dir");
    let (first_model, second_model) = (Jittery::new(0.9), Jittery::new(0.1));
    let first = ledgered(&first_model, &ledger(&dir));
    let second = ledgered(&second_model, &ledger(&dir));
    let asked = request(&["a", "b"]);
    let winner = first.evaluate(&asked).await.expect("first").answers;
    // The second process shares the file: what the first judged is not
    // asked again, and reads the first's bits.
    let fresh = request(&["a", "b", "c"]);
    let late = second.evaluate(&fresh).await.expect("second").answers;
    assert_eq!(late["p0"], winner["p0"]);
    assert_eq!(late["p1"], winner["p1"]);
    assert_eq!(second_model.asked(), vec![vec!["p2".to_string()]]);
}

#[tokio::test]
async fn concurrent_identical_judgements_ask_once() {
    let dir = tempfile::tempdir().expect("dir");
    let model = Arc::new(Jittery {
        yes: 0.5,
        delay: Duration::from_millis(80),
        asked: Mutex::new(Vec::new()),
    });
    let judged = Arc::new(ledgered(&model, &ledger(&dir)));
    let tasks = (0..4)
        .map(|_| {
            let judged = Arc::clone(&judged);
            tokio::spawn(async move { judged.evaluate_traced(&request(&["a", "b"])).await })
        })
        .collect::<Vec<_>>();
    let mut outcomes = Vec::new();
    for task in tasks {
        let (response, origin) = task.await.expect("task");
        outcomes.push((response.expect("answers"), origin));
    }
    assert_eq!(model.asked().len(), 1);
    let sent = outcomes
        .iter()
        .map(|(response, _)| response.requests)
        .sum::<usize>();
    assert_eq!(sent, 1, "only the caller that asked reports the request");
    for (response, _) in &outcomes {
        assert_eq!(response.answers, outcomes[0].0.answers);
    }
}

#[tokio::test]
async fn choices_round_trip_through_the_book() {
    let dir = tempfile::tempdir().expect("dir");
    let model = Jittery::new(0.2);
    let judged = ledgered(&model, &ledger(&dir));
    let asked = JudgementRequest {
        state: json!({"facts": ["one", "two"]}),
        questions: BTreeMap::from([(
            "c0".to_string(),
            JudgementQuestion::Choice {
                instructions: json!("Which relation?"),
                options: vec!["supports".into(), "none".into(), "answers".into()],
            },
        )]),
    };
    let first = judged.evaluate(&asked).await.expect("first");
    let (again, origin) = judged.evaluate_traced(&asked).await;
    assert_eq!(origin, JudgementOrigin::BookHit);
    let again = again.expect("again");
    assert_eq!(again.answers, first.answers);
    let Some(JudgementAnswer::Choice { choice, .. }) = again.answers.get("c0") else {
        panic!("choice")
    };
    assert_eq!(choice, "answers");
}

/// A book whose storage is gone.
struct Broken;

impl VerdictBook for Broken {
    fn read(&self, _: &[VerdictKey]) -> Result<Vec<Option<Verdict>>, String> {
        Err("disk gone".into())
    }
    fn record(&self, _: &[(VerdictKey, Verdict)]) -> Result<Vec<Verdict>, String> {
        Err("disk gone".into())
    }
    fn invalidate(&self, _: &[u8]) -> Result<usize, String> {
        Err("disk gone".into())
    }
}

#[tokio::test]
async fn a_broken_book_never_stops_judging() {
    let model = Jittery::new(0.4);
    let judged = LedgeredJudgement::new(
        Arc::clone(&model),
        Arc::new(VerdictLedger::new(Arc::new(Broken))),
        JudgementSite::CurateReview,
    );
    let asked = request(&["a"]);
    let first = judged.evaluate(&asked).await.expect("first");
    let second = judged.evaluate(&asked).await.expect("second");
    assert_eq!(first.answers, second.answers);
    assert_eq!(model.asked().len(), 2);
}

#[tokio::test]
async fn an_empty_request_and_a_failure_pass_through_and_keep_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let ledger = ledger(&dir);
    let model = Jittery::new(0.4);
    let judged = ledgered(&model, &ledger);
    let empty = judged.evaluate(&request(&[])).await.expect("empty");
    assert!(empty.answers.is_empty());

    struct Failing;
    impl JudgementModel for Failing {
        fn model(&self) -> &str {
            "jev-test"
        }
        fn evaluate<'a>(
            &'a self,
            _: &'a JudgementRequest,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>>
        {
            Box::pin(async { Err("TypeSafe timed out".to_string()) })
        }
    }
    let failing = LedgeredJudgement::new(Failing, Arc::clone(&ledger), JudgementSite::Rerank);
    assert_eq!(
        failing.evaluate(&request(&["z"])).await.err().as_deref(),
        Some("TypeSafe timed out")
    );
    let (_, origin) = judged.evaluate_traced(&request(&["z"])).await;
    assert_eq!(
        origin,
        JudgementOrigin::Remote,
        "the failure kept no verdict"
    );
}

#[tokio::test]
async fn behind_a_cassette_a_partly_known_request_goes_whole_and_hits_keep_the_book() {
    let dir = tempfile::tempdir().expect("dir");
    let book =
        SqliteVerdictBook::open(&dir.path().join("judgements.sqlite3"), 1 << 20).expect("book");
    let ledger = Arc::new(VerdictLedger::new(Arc::new(book)).sending_whole_requests(true));
    let first_model = Jittery::new(0.3);
    let first = ledgered(&first_model, &ledger)
        .evaluate(&request(&["a"]))
        .await
        .expect("first");
    let second_model = Jittery::new(0.8);
    let (both, origin) = ledgered(&second_model, &ledger)
        .evaluate_traced(&request(&["a", "new"]))
        .await;
    let both = both.expect("second");
    assert_eq!(origin, JudgementOrigin::Remote);
    assert_eq!(
        second_model.asked(),
        vec![vec!["p0".to_string(), "p1".into()]],
        "the body a binary without the book would send"
    );
    assert_eq!(
        both.answers["p0"], first.answers["p0"],
        "the first verdict wins"
    );
}
