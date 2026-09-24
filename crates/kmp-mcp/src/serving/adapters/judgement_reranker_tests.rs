use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use kmp_proto_mapping::v1beta1::SemanticSource;
use sha2::{Digest, Sha256};

use super::judgement_reranker::JudgementReranker;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// Says yes to passages that contain `word`, and counts its calls.
struct KeywordJudge {
    word: &'static str,
    fail: bool,
    calls: Mutex<usize>,
}

impl JudgementModel for KeywordJudge {
    fn model(&self) -> &str {
        "jev-test"
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
            .iter()
            .map(|(key, question)| {
                let yes = match question {
                    JudgementQuestion::Noul { instructions } => {
                        if instructions["passage"]
                            .as_str()
                            .unwrap_or_default()
                            .contains(self.word)
                        {
                            0.9
                        } else {
                            0.1
                        }
                    }
                    JudgementQuestion::Choice { .. } => 0.0,
                };
                (key.clone(), JudgementAnswer::Noul { yes })
            })
            .collect::<BTreeMap<_, _>>();
        let fail = self.fail;
        Box::pin(async move {
            if fail {
                return Err("TypeSafe unavailable".into());
            }
            Ok(JudgementResponse {
                model: "jev-test".into(),
                answers,
                input_tokens: 1,
                requests: 1,
            })
        })
    }
}

fn source(entry_ref: &str, text: &str) -> SemanticSource {
    SemanticSource {
        entry_ref: entry_ref.into(),
        text: text.into(),
        text_sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
    }
}

fn judge(fail: bool) -> Arc<KeywordJudge> {
    Arc::new(KeywordJudge {
        word: "repaired",
        fail,
        calls: Mutex::new(0),
    })
}

#[tokio::test]
async fn passages_are_ordered_by_the_judgement_and_frozen_for_pages() {
    let model = judge(false);
    let reranker = JudgementReranker::new(model.clone(), 40);
    let pool = vec![
        source("entry:a", "The car was fixed by Ana."),
        source("entry:b", "A technician repaired the automobile."),
    ];
    let first = reranker
        .rank("Who fixed the car?", &pool, false)
        .await
        .expect("first");
    assert!(first.warning.is_none());
    let ranking = first.ranking.expect("ranking");
    let order = format!("{ranking:?}");
    assert!(order.contains("entry:b"), "{order}");
    assert!(
        !order.contains("entry:a"),
        "a passage judged unlikely to answer stays out: {order}"
    );
    let page = reranker
        .rank("Who fixed the car?", &pool, true)
        .await
        .expect("page");
    assert!(page.ranking.is_some());
    assert_eq!(
        *model.calls.lock().expect("calls"),
        1,
        "a continuation never calls again"
    );
}

#[tokio::test]
async fn a_failure_is_frozen_as_a_warning_and_an_unknown_page_is_refused() {
    let reranker = JudgementReranker::new(judge(true), 40);
    let pool = vec![source("entry:a", "text")];
    let outcome = reranker.rank("q", &pool, false).await.expect("outcome");
    assert!(outcome.ranking.is_none());
    assert!(
        outcome
            .warning
            .expect("warning")
            .contains("ordinary retrieval")
    );
    let refused = reranker.rank("another", &pool, true).await;
    assert!(refused.is_err());
}

#[test]
fn loading_needs_rerank_json_and_a_working_typesafe_opt_in() {
    let dir = tempfile::tempdir().expect("dir");
    let model: Arc<dyn JudgementModel> = judge(false);
    assert!(
        JudgementReranker::load(dir.path(), &Ok(Some(model.clone())))
            .expect("off")
            .is_none()
    );
    std::fs::write(dir.path().join("rerank.json"), r#"{"pool_size":8}"#).expect("config");
    assert!(
        JudgementReranker::load(dir.path(), &Ok(None))
            .err()
            .expect("needs typesafe")
            .contains("typesafe.json")
    );
    assert!(
        JudgementReranker::load(dir.path(), &Err("TYPESAFE_API_KEY is not set".into()))
            .err()
            .expect("key")
            .contains("TYPESAFE_API_KEY")
    );
    let loaded = JudgementReranker::load(dir.path(), &Ok(Some(model)))
        .expect("loads")
        .expect("on");
    assert_eq!(loaded.pool_size(), 8);
}
