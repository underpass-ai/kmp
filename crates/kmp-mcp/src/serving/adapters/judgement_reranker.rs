use std::path::Path;
use std::sync::Arc;

use std::time::Duration;

use kmp_proto_mapping::v1beta1::{LexicalMargin, RerankCandidateRanking, SemanticSource};
use sha2::{Digest, Sha256};

use super::passage_judgement::judge_passages;
use super::rerank_config::RerankConfig;
use super::shared_outcomes::SharedOutcomes;
use crate::serving::environment::judgement_deadline;
use crate::serving::judgement_site::JudgementSite;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::rerank_outcome::RerankOutcome;

/// At most this many passages enter the ranking, best first.
const RANKED: usize = 100;
/// Frozen selections kept for continuation pages.
const KEPT: usize = 64;
/// Passages Jev judges less likely than this to answer are left out of the
/// ranking: a channel that listed the whole pool would grow proof with what
/// the judge itself thinks is irrelevant.
const ANSWERS_AT: f64 = 0.5;

/// Ask re-ranking by a remote judgement model, behind its own opt-in. One
/// yes/no question per admitted passage — does it answer the question? —
/// ordered best first. Frozen outcomes include failures, so a continuation
/// can never read a different order than its first page. Concurrent asks
/// for the same selection share one judgement; different selections judge
/// in parallel. A first page waits at most the site's deadline, then reads
/// ordinary retrieval with a warning while the judgement finishes in the
/// background into the verdict book.
pub(super) struct JudgementReranker {
    model: Arc<dyn JudgementModel>,
    pool_size: usize,
    excerpt_chars: usize,
    /// The margin gate's threshold in tenths, or `None` when it is off.
    margin_tenths: Option<i64>,
    deadline: Option<Duration>,
    selections: SharedOutcomes<RerankOutcome>,
}

impl JudgementReranker {
    /// Off without `rerank.json`. With it, the store's TypeSafe opt-in must
    /// work too; otherwise the error says why and Ask stays ordinary.
    pub(super) fn load(
        data_dir: &Path,
        judgement: &Result<Option<Arc<dyn JudgementModel>>, String>,
    ) -> Result<Option<Arc<Self>>, String> {
        let bytes = match std::fs::read(data_dir.join("rerank.json")) {
            Ok(bytes) if bytes.len() <= 8192 => bytes,
            Ok(_) => return Err("rerank configuration exceeds 8192 bytes".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("cannot read rerank configuration".into()),
        };
        let config: RerankConfig =
            serde_json::from_slice(&bytes).map_err(|_| "invalid rerank configuration")?;
        let (pool_size, excerpt_chars) = config.validate()?;
        match judgement {
            Ok(Some(model)) => Ok(Some(Arc::new(
                Self::new(Arc::clone(model), pool_size, excerpt_chars)
                    .with_margin(config.margin_tenths),
            ))),
            Ok(None) => Err("rerank.json needs typesafe.json beside the store".into()),
            Err(error) => Err(error.clone()),
        }
    }

    pub(super) fn new(
        model: Arc<dyn JudgementModel>,
        pool_size: usize,
        excerpt_chars: usize,
    ) -> Self {
        Self {
            model,
            pool_size,
            excerpt_chars,
            margin_tenths: None,
            deadline: judgement_deadline(JudgementSite::Rerank),
            selections: SharedOutcomes::new(KEPT),
        }
    }

    /// The same reranker with another first-page deadline (tests).
    #[cfg(test)]
    pub(super) fn with_deadline(mut self, deadline: Option<Duration>) -> Self {
        self.deadline = deadline;
        self
    }

    /// The same reranker behind the margin gate at `tenths`, or without it.
    pub(super) fn with_margin(mut self, tenths: Option<i64>) -> Self {
        self.margin_tenths = tenths;
        self
    }

    pub(super) fn pool_size(&self) -> usize {
        self.pool_size
    }

    /// Whether the lexical ranking settles this Ask on its own: its lead is
    /// at least the threshold and its confidence high (DESIGN L4 4c).
    pub(super) fn is_settled(&self, margin: Option<LexicalMargin>) -> bool {
        self.margin_tenths
            .zip(margin)
            .is_some_and(|(tau, margin)| margin.is_decisive(tau))
    }

    pub(super) async fn rank(
        &self,
        question: &str,
        pool: &[SemanticSource],
        continuation: bool,
    ) -> Result<RerankOutcome, String> {
        let key = selection_key(self.model.model(), question, pool);
        self.selections
            .get_or_run(&key, !continuation, || self.judge_in_time(question, pool))
            .await
            .map(|(outcome, _)| outcome)
            .ok_or_else(|| {
                "evidence rerank selection expired or its source snapshot changed; start a fresh ask"
                    .into()
            })
    }

    async fn judge_in_time(&self, question: &str, pool: &[SemanticSource]) -> RerankOutcome {
        let model = Arc::clone(&self.model);
        let (question, pool, excerpt_chars) =
            (question.to_string(), pool.to_vec(), self.excerpt_chars);
        // Detached, so a judgement past the deadline still lands in the book
        // for the next ask.
        let judged = tokio::spawn(async move {
            let kept = judge_passages(
                model.as_ref(),
                &question,
                "Does `passage` answer the question in the state?",
                &pool,
                excerpt_chars,
                ANSWERS_AT,
                RANKED,
            )
            .await?;
            RerankCandidateRanking::new(model.model().to_string(), &question, kept)
                .map_err(|_| "invalid rerank identities".to_string())
        });
        let joined = match self.deadline {
            Some(deadline) => match tokio::time::timeout(deadline, judged).await {
                Ok(joined) => joined,
                Err(_) => {
                    return unavailable(format!(
                        "Jev did not answer within {} ms",
                        deadline.as_millis()
                    ));
                }
            },
            None => judged.await,
        };
        match joined {
            Ok(Ok(ranking)) => RerankOutcome {
                ranking: Some(ranking),
                warning: None,
            },
            Ok(Err(error)) => unavailable(error),
            Err(_) => unavailable("the judgement task failed".into()),
        }
    }
}

fn unavailable(error: String) -> RerankOutcome {
    RerankOutcome {
        ranking: None,
        warning: Some(format!(
            "evidence rerank unavailable; using ordinary retrieval: {error}"
        )),
    }
}

fn selection_key(model: &str, question: &str, pool: &[SemanticSource]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"kmp.rerank.selection.v1\0");
    hasher.update(model.as_bytes());
    hasher.update(b"\0");
    hasher.update(question.as_bytes());
    for source in pool {
        hasher.update(b"\0");
        hasher.update(source.entry_ref.as_bytes());
        hasher.update(b"\0");
        hasher.update(source.text_sha256.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}
