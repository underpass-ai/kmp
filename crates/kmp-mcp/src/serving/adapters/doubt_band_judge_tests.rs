use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kmp_proto_mapping::v1beta1::{DoubtBand, DoubtEntry, DoubtJudgement, DoubtPassage};
use sha2::{Digest, Sha256};

use super::ask_judge_config::AskJudgeConfig;
use super::doubt_band_judge::{DoubtBandJudge, doubt_judgement};
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// Answers every passage that contains `word` as answering, keeps the
/// requests it was sent, and may fail or sleep.
struct PassageJudge {
    word: &'static str,
    fail: bool,
    sleep: Option<Duration>,
    asked: Mutex<Vec<JudgementRequest>>,
}

impl JudgementModel for PassageJudge {
    fn model(&self) -> &str {
        "jev-test"
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>,
    > {
        self.asked.lock().expect("asked").push(request.clone());
        let answers = request
            .questions
            .iter()
            .map(|(key, question)| {
                let (instructions, graded) = match question {
                    JudgementQuestion::Noul { instructions } => (instructions, false),
                    JudgementQuestion::Score { instructions, .. } => (instructions, true),
                    JudgementQuestion::Choice { instructions, .. } => (instructions, false),
                };
                let hit = instructions["passage"]
                    .as_str()
                    .unwrap_or_default()
                    .contains(self.word);
                let answer = if graded {
                    JudgementAnswer::Score {
                        score: if hit { 0.1 } else { 2.9 },
                        probabilities: if hit {
                            vec![0.9, 0.1, 0.0, 0.0]
                        } else {
                            vec![0.0, 0.1, 0.0, 0.9]
                        },
                        confidence: 0.9,
                    }
                } else {
                    JudgementAnswer::Noul {
                        yes: if hit { 0.95 } else { 0.05 },
                    }
                };
                (key.clone(), answer)
            })
            .collect::<BTreeMap<_, _>>();
        let (fail, sleep) = (self.fail, self.sleep);
        Box::pin(async move {
            if let Some(sleep) = sleep {
                tokio::time::sleep(sleep).await;
            }
            if fail {
                return Err("TypeSafe unavailable".into());
            }
            Ok(JudgementResponse {
                model: "jev-test".into(),
                answers,
                input_tokens: 100,
                requests: 1,
                elapsed_us: 0,
            })
        })
    }
}

fn model(fail: bool, sleep: Option<Duration>) -> Arc<PassageJudge> {
    Arc::new(PassageJudge {
        word: "6380",
        fail,
        sleep,
        asked: Mutex::new(Vec::new()),
    })
}

fn config(text: &str) -> AskJudgeConfig {
    let config: AskJudgeConfig = serde_json::from_str(text).expect("config");
    config.validate().expect("valid");
    config
}

fn passage(id: &str, text: &str, in_core: bool) -> DoubtPassage {
    DoubtPassage {
        id: id.into(),
        entry_ref: id.into(),
        text: text.into(),
        text_sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
        in_core,
        promotable: true,
    }
}

fn band(count: usize) -> DoubtBand {
    DoubtBand {
        entry: DoubtEntry::NarrowMargin,
        margin: Some(3),
        passages: (0..count)
            .map(|n| {
                passage(
                    &format!("entry:{n}"),
                    &format!(
                        "{n}: the cache listens on port {}",
                        if n == 0 { 6380 } else { 80 }
                    ),
                    n == 0,
                )
            })
            .collect(),
    }
}

#[tokio::test]
async fn one_batch_asks_about_every_passage_and_keeps_every_verdict() {
    let model = model(false, None);
    let judge = DoubtBandJudge::new(model.clone(), config(r#"{"excerpt_chars": 200}"#));
    let outcome = judge.judge("Which port?", &band(3)).await;
    assert!(outcome.warning.is_none());
    let verdicts = outcome.verdicts.expect("verdicts");
    assert_eq!(verdicts.len(), 3);
    assert_eq!(verdicts.entry(), DoubtEntry::NarrowMargin);
    let asked = model.asked.lock().expect("asked");
    assert_eq!(asked.len(), 1, "one batch");
    assert_eq!(
        asked[0].questions.keys().cloned().collect::<Vec<_>>(),
        ["d0", "d1", "d2"]
    );
    assert_eq!(asked[0].state, serde_json::json!("Which port?"));
    assert!(matches!(
        asked[0].questions["d0"],
        JudgementQuestion::Noul { .. }
    ));
}

#[tokio::test]
async fn the_graded_question_asks_the_four_levels_in_order() {
    let model = model(false, None);
    let judge = DoubtBandJudge::new(model.clone(), config(r#"{"question": "score"}"#));
    let outcome = judge.judge("Which port?", &band(2)).await;
    assert_eq!(outcome.verdicts.expect("verdicts").len(), 2);
    let asked = model.asked.lock().expect("asked");
    let JudgementQuestion::Score { levels, .. } = &asked[0].questions["d1"] else {
        panic!("a graded question")
    };
    assert_eq!(
        levels,
        &["answers", "partly", "related_not_answering", "unrelated"]
    );
}

#[test]
fn answers_read_as_thousandths_of_answering_and_of_not() {
    assert_eq!(
        doubt_judgement(&JudgementAnswer::Noul { yes: 0.8126 }),
        Some(DoubtJudgement {
            answers: 813,
            not_answers: 187
        })
    );
    assert_eq!(
        doubt_judgement(&JudgementAnswer::Score {
            score: 1.2,
            probabilities: vec![0.3, 0.4, 0.2, 0.1],
            confidence: 0.5
        }),
        Some(DoubtJudgement {
            answers: 300,
            not_answers: 300
        }),
        "in part is neither answering nor not"
    );
    assert_eq!(
        doubt_judgement(&JudgementAnswer::Score {
            score: 0.0,
            probabilities: vec![1.0, 0.0],
            confidence: 1.0
        }),
        None,
        "another scale"
    );
}

#[tokio::test]
async fn a_failure_or_a_late_judge_leaves_the_deterministic_answer_warned() {
    let judge = DoubtBandJudge::new(model(true, None), config("{}"));
    let outcome = judge.judge("Which port?", &band(2)).await;
    assert!(outcome.verdicts.is_none());
    assert!(
        outcome
            .warning
            .expect("warned")
            .starts_with("doubt band unavailable; the deterministic answer stands")
    );

    let slow = model(false, Some(Duration::from_millis(200)));
    let judge = DoubtBandJudge::new(slow.clone(), config("{}"))
        .with_deadline(Some(Duration::from_millis(10)));
    let outcome = judge.judge("Which port?", &band(2)).await;
    assert!(outcome.verdicts.is_none());
    assert!(outcome.warning.expect("warned").contains("within 10 ms"));
    // The judgement goes on in the background, into the verdict book.
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(slow.asked.lock().expect("asked").len(), 1);
}

#[test]
fn loading_needs_ask_judge_json_and_a_working_typesafe_opt_in() {
    let dir = tempfile::tempdir().expect("dir");
    let model: Arc<dyn JudgementModel> = model(false, None);
    assert!(
        DoubtBandJudge::load(dir.path(), &Ok(Some(Arc::clone(&model))))
            .expect("no file")
            .is_none()
    );
    std::fs::write(dir.path().join("ask-judge.json"), "{}").expect("config");
    assert!(
        DoubtBandJudge::load(dir.path(), &Ok(None))
            .err()
            .expect("needs typesafe")
            .contains("typesafe.json")
    );
    let judge = DoubtBandJudge::load(dir.path(), &Ok(Some(Arc::clone(&model))))
        .expect("loads")
        .expect("on");
    assert_eq!(judge.margin_tenths(), 10);
    std::fs::write(dir.path().join("ask-judge.json"), r#"{"veto_at": 2}"#).expect("config");
    assert!(DoubtBandJudge::load(dir.path(), &Ok(Some(model))).is_err());
    std::fs::write(dir.path().join("ask-judge.json"), "{}").expect("config");
    assert_eq!(
        DoubtBandJudge::load(dir.path(), &Err("broken typesafe".into()))
            .err()
            .expect("the opt-in's error passes through"),
        "broken typesafe"
    );
}
