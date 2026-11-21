//! Structured telemetry for every judgement a store spends.
//!
//! One line per `evaluate`, on target `kmp_mcp::judgement` at debug level,
//! so it costs nothing unless an operator asks for it
//! (`RUST_LOG=kmp_mcp=info,kmp_mcp::judgement=debug`). The line carries
//! counts, the cassette key and the time taken — never the state, the
//! questions or the answers. The format is a contract read by the memory
//! bench: `scripts/performance/memory_bench/SCHEMAS.md`, section telemetry.

use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::judgement_key::judgement_key;
use super::ledgered_judgement::LedgeredJudgement;
use super::verdict_ledger::VerdictLedger;
use crate::serving::environment::judgement_deadline;
use crate::serving::judgement_origin::JudgementOrigin;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::judgement_site::JudgementSite;
use crate::serving::ports::judgement_model::JudgementModel;

const TARGET: &str = "kmp_mcp::judgement";

/// A judgement model that reports, per call, which site spent it, how many
/// questions and provider requests it took, its input tokens, where the
/// answers came from and how long it took. Answers pass through unchanged.
///
/// The sites a committed write asks (relation proposals, search expansions)
/// wait at most their deadline here, per judgement: past it the call fails
/// as "not judged", never as a no, and runs on detached so its verdict
/// still lands in the book. Read sites keep their own first-page deadline.
pub(super) struct ObservedJudgement {
    inner: Arc<dyn JudgementModel>,
    site: JudgementSite,
    deadline: Option<Duration>,
}

impl ObservedJudgement {
    pub(super) fn new(inner: Arc<dyn JudgementModel>, site: JudgementSite) -> Self {
        let deadline = matches!(
            site,
            JudgementSite::WriteRelations | JudgementSite::Expansions
        )
        .then(|| judgement_deadline(site))
        .flatten();
        Self {
            inner,
            site,
            deadline,
        }
    }

    /// The same judgement with another write deadline (tests).
    #[cfg(test)]
    pub(super) fn with_deadline(mut self, deadline: Option<Duration>) -> Self {
        self.deadline = deadline;
        self
    }

    /// The store's judgement for one site: behind its verdict book when it
    /// has one, and observed; absence and the reason a present opt-in
    /// cannot run pass through.
    pub(super) fn for_site(
        judgement: &Result<Option<Arc<dyn JudgementModel>>, String>,
        ledger: Option<&Arc<VerdictLedger>>,
        site: JudgementSite,
    ) -> Result<Option<Arc<dyn JudgementModel>>, String> {
        judgement.clone().map(|model| {
            model.map(|model| {
                Arc::new(Self::at_site(&model, ledger, site)) as Arc<dyn JudgementModel>
            })
        })
    }

    /// `model` behind the book when there is one, observed for `site`.
    pub(super) fn at_site(
        model: &Arc<dyn JudgementModel>,
        ledger: Option<&Arc<VerdictLedger>>,
        site: JudgementSite,
    ) -> Self {
        Self::new(LedgeredJudgement::in_front_of(model, ledger, site), site)
    }

    fn report(
        &self,
        request: &JudgementRequest,
        outcome: &Result<JudgementResponse, String>,
        origin: JudgementOrigin,
        started: Instant,
    ) {
        if !tracing::enabled!(target: TARGET, tracing::Level::DEBUG) {
            return;
        }
        let elapsed_us = started.elapsed().as_micros() as u64;
        let request_key = judgement_key(self.inner.model(), request);
        let questions = request.questions.len();
        match outcome {
            Ok(response) => tracing::debug!(
                target: TARGET,
                event = "kmp_judgement",
                site = self.site.as_str(),
                model = response.model.as_str(),
                source = origin.as_str(),
                status = "ok",
                questions,
                answers = response.answers.len(),
                requests = response.requests,
                http_requests = origin.http_requests(response.requests),
                input_tokens = response.input_tokens,
                elapsed_us,
                request_key = request_key.as_str(),
                "judgement evaluated"
            ),
            Err(error) => tracing::debug!(
                target: TARGET,
                event = "kmp_judgement",
                site = self.site.as_str(),
                model = self.inner.model(),
                source = origin.as_str(),
                status = "error",
                questions,
                elapsed_us,
                request_key = request_key.as_str(),
                error_hash = error_hash(error).as_str(),
                "judgement failed"
            ),
        }
    }
}

/// A stable, short hash of a failure, so lines group by cause without the
/// message (which may quote stored text) reaching telemetry.
fn error_hash(message: &str) -> String {
    format!("{:x}", Sha256::digest(message.as_bytes()))
        .chars()
        .take(16)
        .collect()
}

impl JudgementModel for ObservedJudgement {
    fn model(&self) -> &str {
        self.inner.model()
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>>
    {
        Box::pin(async move { self.evaluate_traced(request).await.0 })
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
        Box::pin(async move {
            let started = Instant::now();
            let (mut outcome, origin) = match self.deadline {
                None => self.inner.evaluate_traced(request).await,
                Some(deadline) => {
                    let (inner, asked) = (Arc::clone(&self.inner), request.clone());
                    // Detached: a late judgement still lands in the book.
                    let judged = tokio::spawn(async move { inner.evaluate_traced(&asked).await });
                    match tokio::time::timeout(deadline, judged).await {
                        Ok(Ok(traced)) => traced,
                        Ok(Err(_)) => (
                            Err("not judged: the judgement task failed".to_string()),
                            JudgementOrigin::Remote,
                        ),
                        Err(_) => (
                            Err(format!(
                                "not judged: Jev did not answer within {} ms",
                                deadline.as_millis()
                            )),
                            JudgementOrigin::Remote,
                        ),
                    }
                }
            };
            if let Ok(response) = &mut outcome {
                response.elapsed_us = started.elapsed().as_micros() as u64;
            }
            self.report(request, &outcome, origin, started);
            (outcome, origin)
        })
    }
}
