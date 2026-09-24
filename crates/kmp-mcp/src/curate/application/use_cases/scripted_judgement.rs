use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// Answers every noul with `noul` and every choice with `choice` when
/// offered (else `none`), at `confidence`.
pub(crate) struct Scripted {
    pub noul: f64,
    pub choice: &'static str,
    pub confidence: f64,
    pub calls: Mutex<usize>,
}

impl JudgementModel for Scripted {
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
                let answer = match question {
                    JudgementQuestion::Noul { .. } => JudgementAnswer::Noul { yes: self.noul },
                    JudgementQuestion::Choice { options, .. } => {
                        let choice = if options.iter().any(|o| o == self.choice) {
                            self.choice
                        } else {
                            "none"
                        };
                        JudgementAnswer::Choice {
                            choice: choice.into(),
                            probabilities: options
                                .iter()
                                .map(|o| {
                                    (o.clone(), if o == choice { self.confidence } else { 0.0 })
                                })
                                .collect(),
                            confidence: self.confidence,
                        }
                    }
                };
                (key.clone(), answer)
            })
            .collect::<BTreeMap<_, _>>();
        Box::pin(async move {
            Ok(JudgementResponse {
                model: "jev-test".into(),
                answers,
                input_tokens: 10,
                requests: 1,
            })
        })
    }
}
