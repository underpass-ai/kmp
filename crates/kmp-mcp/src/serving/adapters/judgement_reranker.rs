use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use kmp_proto_mapping::v1beta1::{RerankCandidateRanking, SemanticSource};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::rerank_config::RerankConfig;
use crate::serving::judgement_answer::JudgementAnswer;
use crate::serving::judgement_question::JudgementQuestion;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::rerank_outcome::RerankOutcome;

/// Text of one passage sent for judgement, in characters.
const SENT_CHARS: usize = 2_000;
/// Frozen selections kept for continuation pages.
const KEPT: usize = 64;

/// Ask re-ranking by a remote judgement model, behind its own opt-in. One
/// yes/no question per admitted passage — does it answer the question? —
/// ordered best first. Frozen outcomes include failures, so a continuation
/// can never read a different order than its first page.
pub(super) struct JudgementReranker {
    model: Arc<dyn JudgementModel>,
    pool_size: usize,
    selections: Mutex<BTreeMap<String, RerankOutcome>>,
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
        let pool_size = config.validate()?;
        match judgement {
            Ok(Some(model)) => Ok(Some(Arc::new(Self::new(Arc::clone(model), pool_size)))),
            Ok(None) => Err("rerank.json needs typesafe.json beside the store".into()),
            Err(error) => Err(error.clone()),
        }
    }

    pub(super) fn new(model: Arc<dyn JudgementModel>, pool_size: usize) -> Self {
        Self {
            model,
            pool_size,
            selections: Mutex::new(BTreeMap::new()),
        }
    }

    pub(super) fn pool_size(&self) -> usize {
        self.pool_size
    }

    pub(super) async fn rank(
        &self,
        question: &str,
        pool: &[SemanticSource],
        continuation: bool,
    ) -> Result<RerankOutcome, String> {
        let key = selection_key(self.model.model(), question, pool);
        let mut selections = self.selections.lock().await;
        if let Some(outcome) = selections.get(&key) {
            return Ok(outcome.clone());
        }
        if continuation {
            return Err(
                "evidence rerank selection expired or its source snapshot changed; start a fresh ask"
                    .into(),
            );
        }
        let outcome = match self.judge(question, pool).await {
            Ok(ranking) => RerankOutcome {
                ranking: Some(ranking),
                warning: None,
            },
            Err(error) => RerankOutcome {
                ranking: None,
                warning: Some(format!(
                    "evidence rerank unavailable; using ordinary retrieval: {error}"
                )),
            },
        };
        if selections.len() >= KEPT {
            selections.pop_first();
        }
        selections.insert(key, outcome.clone());
        Ok(outcome)
    }

    async fn judge(
        &self,
        question: &str,
        pool: &[SemanticSource],
    ) -> Result<RerankCandidateRanking, String> {
        let questions = pool
            .iter()
            .enumerate()
            .map(|(n, source)| {
                (
                    format!("p{n}"),
                    JudgementQuestion::Noul {
                        instructions: json!({
                            "passage": source.text.chars().take(SENT_CHARS).collect::<String>(),
                            "question": "Does `passage` answer the question in the state?",
                        }),
                    },
                )
            })
            .collect();
        let response = self
            .model
            .evaluate(&JudgementRequest {
                state: json!(question),
                questions,
            })
            .await?;
        let mut scored = pool
            .iter()
            .enumerate()
            .map(|(n, source)| {
                let yes = match response.answers.get(&format!("p{n}")) {
                    Some(JudgementAnswer::Noul { yes }) => *yes,
                    _ => 0.0,
                };
                (yes, source)
            })
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.1.entry_ref.cmp(&right.1.entry_ref))
        });
        RerankCandidateRanking::new(
            self.model.model().to_string(),
            question,
            scored
                .into_iter()
                .map(|(_, source)| (source.entry_ref.clone(), source.text_sha256.clone()))
                .collect(),
        )
        .map_err(|_| "invalid rerank identities".to_string())
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
