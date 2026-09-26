use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::cassette_judgement::judgement_key;
use super::verdict_ledger::VerdictLedger;
use crate::serving::judgement_origin::JudgementOrigin;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::judgement_site::JudgementSite;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::verdict::Verdict;
use crate::serving::verdict_key::VerdictKey;
use crate::serving::verdict_state_digest::StateDigest;
use crate::serving::verdict_template::VerdictTemplate;

#[cfg(test)]
#[path = "ledgered_judgement_tests.rs"]
mod tests;

/// A judgement model in front of the store's verdict book (DESIGN L4 4a).
/// Each question is looked up by its verdict key; only the questions the
/// book lacks reach the model, in one request that keeps the state. Answers
/// are quantized once, when they arrive, and the first verdict recorded for
/// a key is what every caller reads, this process or another — so a
/// repeated judgement costs nothing and answers the same bits: Jev frozen
/// per key.
pub(super) struct LedgeredJudgement<M> {
    inner: M,
    ledger: Arc<VerdictLedger>,
    template: VerdictTemplate,
}

impl LedgeredJudgement<Arc<dyn JudgementModel>> {
    /// `model` behind the book when the store has one, as it is otherwise.
    pub(super) fn in_front_of(
        model: &Arc<dyn JudgementModel>,
        ledger: Option<&Arc<VerdictLedger>>,
        site: JudgementSite,
    ) -> Arc<dyn JudgementModel> {
        match ledger {
            Some(ledger) => Arc::new(Self::new(Arc::clone(model), Arc::clone(ledger), site)),
            None => Arc::clone(model),
        }
    }
}

impl<M: JudgementModel> LedgeredJudgement<M> {
    pub(super) fn new(inner: M, ledger: Arc<VerdictLedger>, site: JudgementSite) -> Self {
        Self {
            inner,
            ledger,
            template: site.template(),
        }
    }

    async fn judge(
        &self,
        request: &JudgementRequest,
    ) -> (Result<JudgementResponse, String>, JudgementOrigin) {
        if request.questions.is_empty() {
            return self.inner.evaluate_traced(request).await;
        }
        let model = self.inner.model();
        let state = StateDigest::of(&request.state);
        let keys = request
            .questions
            .values()
            .map(|question| VerdictKey::of(model, self.template, question, &state))
            .collect::<Vec<_>>();
        let known = self.ledger.read(keys.clone()).await;
        let all_keys = keys.clone();
        let mut answers = BTreeMap::new();
        let mut missing = BTreeMap::new();
        let mut missing_keys = Vec::new();
        for ((name, question), (key, verdict)) in
            request.questions.iter().zip(keys.into_iter().zip(known))
        {
            match verdict.and_then(|verdict| verdict.answer(question)) {
                Some(answer) => {
                    answers.insert(name.clone(), answer);
                }
                None => {
                    missing.insert(name.clone(), question.clone());
                    missing_keys.push(key);
                }
            }
        }
        if missing.is_empty() {
            let mut response = JudgementResponse::empty(model);
            response.answers = answers;
            return (Ok(response), JudgementOrigin::BookHit);
        }
        let (asked, missing_keys) = if self.ledger.whole_requests {
            (request.clone(), all_keys)
        } else {
            (
                JudgementRequest {
                    state: request.state.clone(),
                    questions: missing,
                },
                missing_keys,
            )
        };
        let flight = judgement_key(model, &asked);
        let Some(((outcome, origin), ran)) = self
            .ledger
            .in_flight
            .get_or_run(&flight, true, || self.ask_and_keep(&asked, missing_keys))
            .await
        else {
            return (
                Err("verdict ledger refused a new judgement".into()),
                JudgementOrigin::Remote,
            );
        };
        // Another caller asked these very questions a moment ago: its answer
        // is this one's, and this caller sent nothing.
        let (outcome, origin) = if ran {
            (outcome, origin)
        } else {
            (
                outcome.map(|shared| JudgementResponse {
                    input_tokens: 0,
                    requests: 0,
                    ..shared
                }),
                JudgementOrigin::BookHit,
            )
        };
        match outcome {
            Ok(mut fresh) => {
                answers.append(&mut fresh.answers);
                fresh.answers = answers;
                (Ok(fresh), origin)
            }
            Err(error) => (Err(error), origin),
        }
    }

    /// Asks the model for the questions the book lacked, keeps the answers
    /// and returns what the book holds afterwards: when another process
    /// judged a key first, its verdict is the answer.
    async fn ask_and_keep(
        &self,
        asked: &JudgementRequest,
        keys: Vec<VerdictKey>,
    ) -> (Result<JudgementResponse, String>, JudgementOrigin) {
        let (outcome, origin) = self.inner.evaluate_traced(asked).await;
        let Ok(mut response) = outcome else {
            return (outcome, origin);
        };
        let answered = response.answers.len().max(1) as u64;
        let tokens = u32::try_from(response.input_tokens / answered).unwrap_or(u32::MAX);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs() as i64);
        let mut kept = Vec::new();
        let mut entries = Vec::new();
        for ((name, question), key) in asked.questions.iter().zip(keys) {
            let verdict = response
                .answers
                .get(name)
                .and_then(|answer| Verdict::of(question, answer, tokens, now));
            if let Some(verdict) = verdict {
                kept.push((name, question));
                entries.push((key, verdict));
            }
        }
        let held = self.ledger.record(entries).await;
        for ((name, question), verdict) in kept.into_iter().zip(held) {
            if let Some(answer) = verdict.answer(question) {
                response.answers.insert(name.clone(), answer);
            }
        }
        (Ok(response), origin)
    }
}

impl<M: JudgementModel> JudgementModel for LedgeredJudgement<M> {
    fn model(&self) -> &str {
        self.inner.model()
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>>
    {
        Box::pin(async move { self.judge(request).await.0 })
    }

    fn evaluate_traced<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = (Result<JudgementResponse, String>, JudgementOrigin)>
                + Send
                + 'a,
        >,
    > {
        Box::pin(self.judge(request))
    }
}
