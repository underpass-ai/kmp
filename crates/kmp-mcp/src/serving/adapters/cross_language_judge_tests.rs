use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kmp_proto_mapping::v1beta1::{DoubtBand, DoubtEntry, DoubtPassage};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::ask_cross_language_config::AskCrossLanguageConfig;
use super::cross_language_judge::CrossLanguageJudge;
use super::typesafe_request_body::typesafe_request_body;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// Answers a passage that contains `word` as stating the answer, the
/// companion as `companion`; keeps what it was asked, and may fail, sleep
/// or forget the companion.
struct StatingJudge {
    word: &'static str,
    companion: Option<f64>,
    fail: bool,
    sleep: Option<Duration>,
    asked: Mutex<Vec<JudgementRequest>>,
}

impl JudgementModel for StatingJudge {
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
        let mut answers = BTreeMap::new();
        for (key, question) in &request.questions {
            let JudgementQuestion::CriteriaNoul { instructions, .. } = question else {
                panic!("the cross-language judge asks nouls with criteria");
            };
            let yes = if key == "any" {
                match self.companion {
                    Some(yes) => yes,
                    None => continue,
                }
            } else if instructions["passage"]
                .as_str()
                .unwrap_or_default()
                .contains(self.word)
            {
                0.97
            } else {
                0.02
            };
            answers.insert(key.clone(), JudgementAnswer::Noul { yes });
        }
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

fn model(companion: Option<f64>, fail: bool, sleep: Option<Duration>) -> Arc<StatingJudge> {
    Arc::new(StatingJudge {
        word: "Ostrova",
        companion,
        fail,
        sleep,
        asked: Mutex::new(Vec::new()),
    })
}

fn config(text: &str) -> AskCrossLanguageConfig {
    let config: AskCrossLanguageConfig = serde_json::from_str(text).expect("config");
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

fn rescue_band() -> DoubtBand {
    DoubtBand {
        entry: DoubtEntry::CrossLanguageUnknown,
        margin: None,
        passages: vec![
            passage(
                "e1",
                "La cuadrilla del puerto arregló la puerta rota con Pellucid.",
                false,
            ),
            passage(
                "e2",
                "El turno de noche encontró una fuga debajo de Ostrova.",
                false,
            ),
        ],
    }
}

fn veto_band() -> DoubtBand {
    DoubtBand {
        entry: DoubtEntry::TranslatedAnswer,
        margin: None,
        passages: vec![passage(
            "e3",
            "#10072 permite una latencia de 300 ms para svc-007.",
            true,
        )],
    }
}

fn item(passage: &DoubtPassage) -> kmp_proto::v1beta1::MemoryEvidence {
    kmp_proto::v1beta1::MemoryEvidence {
        id: passage.id.clone(),
        text: passage.text.clone(),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_rescue_asks_each_passage_and_the_companion_in_one_batch() {
    let model = model(Some(0.95), false, None);
    let judge = CrossLanguageJudge::new(model.clone(), config(r#"{"rescue": true}"#));
    let band = rescue_band();
    let outcome = judge
        .judge("Where did the night shift find a leak?", &band)
        .await;
    assert!(outcome.warning.is_none(), "{:?}", outcome.warning);
    let verdicts = outcome.verdicts.expect("verdicts");
    assert_eq!(verdicts.entry(), DoubtEntry::CrossLanguageUnknown);
    assert_eq!(verdicts.len(), 2);
    let asked = model.asked.lock().expect("asked");
    assert_eq!(asked.len(), 1, "one batch");
    assert_eq!(
        asked[0].questions.keys().cloned().collect::<Vec<_>>(),
        ["any", "p0", "p1"]
    );
    assert_eq!(
        asked[0].state,
        json!({"user_question": "Where did the night shift find a leak?",
               "user_question_language": "english"})
    );
    // The wire body says what each answer means; the companion reads every
    // passage the judge was sent.
    let body = typesafe_request_body("jev-1.14.0", &asked[0].state, &asked[0].questions);
    assert_eq!(
        body["questions"]["p1"]["criteria"]["false"],
        "the passage is about the same subject but does not state the asked fact"
    );
    assert_eq!(body["questions"]["p1"]["type"], "noul");
    // The language code read is stated as a fact, not hedged.
    assert_eq!(
        body["questions"]["p1"]["instructions"]["passage_language"],
        "spanish"
    );
    assert_eq!(
        body["questions"]["any"]["instructions"]["passages_language"],
        "spanish"
    );
    let wording = body.to_string();
    assert!(!wording.contains(" or "), "one judgement per question");
    assert_eq!(
        body["questions"]["any"]["instructions"]["passages"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
}

#[tokio::test]
async fn the_rescue_names_only_the_likeliest_passage_and_needs_the_companion() {
    let band = rescue_band();
    let judge = CrossLanguageJudge::new(
        model(Some(0.95), false, None),
        config(r#"{"rescue": true}"#),
    );
    let outcome = judge.judge("Where was the leak?", &band).await;
    let verdicts = outcome.verdicts.expect("verdicts");
    let rescued = band
        .passages
        .iter()
        .filter(|passage| verdicts.is_rescued(&item(passage)))
        .map(|passage| passage.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(rescued, ["e2"]);

    // The same passages with a doubtful companion: nothing is rescued.
    let judge =
        CrossLanguageJudge::new(model(Some(0.6), false, None), config(r#"{"rescue": true}"#));
    let verdicts = judge
        .judge("Where was the leak?", &band)
        .await
        .verdicts
        .expect("verdicts");
    assert!(band.passages.iter().all(|p| !verdicts.is_rescued(&item(p))));
}

#[tokio::test]
async fn a_missing_companion_is_a_failed_judgement_and_today_s_answer_stands() {
    let judge = CrossLanguageJudge::new(model(None, false, None), config(r#"{"rescue": true}"#));
    let outcome = judge.judge("Where was the leak?", &rescue_band()).await;
    assert!(outcome.verdicts.is_none());
    assert!(
        outcome
            .warning
            .expect("warning")
            .starts_with("cross-language judge unavailable; the deterministic answer stands")
    );
}

#[tokio::test]
async fn a_veto_asks_about_the_cited_passages_alone() {
    let model = model(Some(0.95), false, None);
    let judge = CrossLanguageJudge::new(model.clone(), config(r#"{"veto": true}"#));
    let band = veto_band();
    let verdicts = judge
        .judge("¿Qué presupuesto de latencia da #10072 a svc-007?", &band)
        .await
        .verdicts
        .expect("verdicts");
    assert_eq!(verdicts.entry(), DoubtEntry::TranslatedAnswer);
    assert!(
        verdicts.is_vetoed(&item(&band.passages[0])),
        "a passage judged not to state it leaves"
    );
    let asked = model.asked.lock().expect("asked");
    assert_eq!(
        asked[0].questions.keys().cloned().collect::<Vec<_>>(),
        ["p0"]
    );
}

#[tokio::test]
async fn a_failure_or_a_slow_judge_leaves_today_s_answer_with_a_warning() {
    let failing =
        CrossLanguageJudge::new(model(Some(0.95), true, None), config(r#"{"veto": true}"#));
    let outcome = failing.judge("q", &veto_band()).await;
    assert!(outcome.verdicts.is_none());
    assert!(
        outcome
            .warning
            .expect("warning")
            .contains("TypeSafe unavailable")
    );

    let slow = CrossLanguageJudge::new(
        model(Some(0.95), false, Some(Duration::from_millis(200))),
        config(r#"{"veto": true}"#),
    )
    .with_deadline(Some(Duration::from_millis(10)));
    let outcome = slow.judge("q", &veto_band()).await;
    assert!(outcome.verdicts.is_none());
    assert!(
        outcome
            .warning
            .expect("warning")
            .contains("did not answer within 10 ms")
    );
}

#[test]
fn the_judge_is_off_without_its_file_and_refused_without_typesafe() {
    let dir = tempfile::tempdir().expect("dir");
    let model: Arc<dyn JudgementModel> = model(Some(0.95), false, None);
    assert!(matches!(
        CrossLanguageJudge::load(dir.path(), &Ok(Some(Arc::clone(&model)))),
        Ok(None)
    ));
    std::fs::write(
        dir.path().join("ask-cross-language.json"),
        r#"{"rescue": true}"#,
    )
    .expect("file");
    let Err(error) = CrossLanguageJudge::load(dir.path(), &Ok(None)) else {
        panic!("a judge without typesafe.json");
    };
    assert!(error.contains("needs typesafe.json"), "{error}");
    assert!(CrossLanguageJudge::load(dir.path(), &Err("no key".into())).is_err());
    let judge = CrossLanguageJudge::load(dir.path(), &Ok(Some(Arc::clone(&model))))
        .expect("loads")
        .expect("a judge");
    assert!(judge.rescue() && !judge.veto());
    assert_eq!(judge.rescue_permille(), 900);
    for text in ["{}", "not json", r#"{"veto": true, "mode": "x"}"#] {
        std::fs::write(dir.path().join("ask-cross-language.json"), text).expect("file");
        assert!(
            CrossLanguageJudge::load(dir.path(), &Ok(Some(Arc::clone(&model)))).is_err(),
            "{text}"
        );
    }
    std::fs::write(dir.path().join("ask-cross-language.json"), vec![b' '; 9000]).expect("file");
    assert!(CrossLanguageJudge::load(dir.path(), &Ok(Some(model))).is_err());
}
