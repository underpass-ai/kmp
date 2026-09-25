use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::json;

use super::cassette_judgement::CassetteJudgement;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// Answers every noul with 0.7 and counts how often it is asked.
struct Counting {
    calls: Mutex<usize>,
}

impl JudgementModel for Counting {
    fn model(&self) -> &str {
        "jev-1.13.0"
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>,
    > {
        *self.calls.lock().expect("calls") += 1;
        let answers = request
            .questions
            .keys()
            .map(|key| (key.clone(), JudgementAnswer::Noul { yes: 0.7 }))
            .collect::<BTreeMap<_, _>>();
        Box::pin(async move {
            Ok(JudgementResponse {
                model: "jev-1.13.0".into(),
                answers,
                input_tokens: 42,
                requests: 1,
            })
        })
    }
}

fn request(text: &str) -> JudgementRequest {
    JudgementRequest {
        state: json!(text),
        questions: BTreeMap::from([(
            "q".to_string(),
            JudgementQuestion::Noul {
                instructions: json!("Does it hold?"),
            },
        )]),
    }
}

#[tokio::test]
async fn a_recorded_judgement_replays_without_the_model_and_an_unrecorded_one_fails() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("jev.cassette.json");
    let inner = Arc::new(Counting {
        calls: Mutex::new(0),
    });
    let recorder = CassetteJudgement::record(&path, inner.clone()).expect("record");
    let recorded = recorder.evaluate(&request("a")).await.expect("recorded");
    recorder.evaluate(&request("a")).await.expect("again");
    assert_eq!(
        *inner.calls.lock().expect("calls"),
        1,
        "a recorded request is not asked twice"
    );

    let replay = CassetteJudgement::replay(&path, "jev-1.13.0".into()).expect("replay");
    assert_eq!(
        replay.evaluate(&request("a")).await.expect("replayed"),
        recorded
    );
    let missing = replay
        .evaluate(&request("b"))
        .await
        .expect_err("not recorded");
    assert!(missing.contains("not in the cassette"), "{missing}");

    let text = std::fs::read_to_string(&path).expect("file");
    assert!(text.contains("kmp.typesafe.cassette.v1"));
    assert!(!text.to_lowercase().contains("bearer"));
}

#[test]
fn a_cassette_answers_only_for_the_model_it_was_recorded_for() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("jev.cassette.json");
    assert!(
        CassetteJudgement::replay(&path, "jev-1.13.0".into()).is_err(),
        "no file"
    );
    std::fs::write(
        &path,
        r#"{"schema":"kmp.typesafe.cassette.v1","model":"jev-1.12.0","entries":{}}"#,
    )
    .expect("file");
    let error = CassetteJudgement::replay(&path, "jev-1.13.0".into())
        .err()
        .expect("other model");
    assert!(error.contains("jev-1.12.0"), "{error}");
    std::fs::write(
        &path,
        r#"{"schema":"other","model":"jev-1.13.0","entries":{}}"#,
    )
    .expect("file");
    assert!(CassetteJudgement::replay(&path, "jev-1.13.0".into()).is_err());
}
