use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use kmp_proto_mapping::v1beta1::{JudgedSelection, SemanticSource};
use sha2::{Digest, Sha256};

use super::passage_judgement::judge_passages;
use super::rerank_config::RerankConfig;
use super::shared_outcomes::SharedOutcomes;
use crate::serving::environment::judgement_deadline;
use crate::serving::judgement_site::JudgementSite;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::wake_focus_outcome::WakeFocusOutcome;

const KEPT: usize = 64;
/// Evidence judged less likely than this to matter for the intent is
/// withheld from the packet.
const MATTERS_AT: f64 = 0.5;

/// A wake focused on what the caller says it is resuming. One yes/no
/// question per admitted evidence entry — does it matter for this intent? —
/// behind its own opt-in, `wake-focus.json`, on top of `typesafe.json`.
/// Frozen per intent, pool and model so continuation pages agree. The first
/// page waits at most the site's deadline, then reads an ordinary wake with
/// a warning while the judgement finishes into the verdict book.
pub(super) struct WakeFocusJudge {
    model: Arc<dyn JudgementModel>,
    pool_size: usize,
    excerpt_chars: usize,
    deadline: Option<Duration>,
    selections: SharedOutcomes<WakeFocusOutcome>,
}

impl WakeFocusJudge {
    pub(super) fn load(
        data_dir: &Path,
        judgement: &Result<Option<Arc<dyn JudgementModel>>, String>,
    ) -> Result<Option<Arc<Self>>, String> {
        let bytes = match std::fs::read(data_dir.join("wake-focus.json")) {
            Ok(bytes) if bytes.len() <= 8192 => bytes,
            Ok(_) => return Err("wake focus configuration exceeds 8192 bytes".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("cannot read wake focus configuration".into()),
        };
        let config: RerankConfig =
            serde_json::from_slice(&bytes).map_err(|_| "invalid wake focus configuration")?;
        let (pool_size, excerpt_chars) = config.validate()?;
        match judgement {
            Ok(Some(model)) => Ok(Some(Arc::new(Self {
                model: Arc::clone(model),
                pool_size,
                excerpt_chars,
                deadline: judgement_deadline(JudgementSite::WakeFocus),
                selections: SharedOutcomes::new(KEPT),
            }))),
            Ok(None) => Err("wake-focus.json needs typesafe.json beside the store".into()),
            Err(error) => Err(error.clone()),
        }
    }

    pub(super) async fn focus(
        &self,
        intent: &str,
        sources: &[SemanticSource],
        continuation: bool,
    ) -> Result<WakeFocusOutcome, String> {
        let pool = &sources[..sources.len().min(self.pool_size)];
        let mut hasher = Sha256::new();
        hasher.update(b"kmp.wake.focus.v1\0");
        hasher.update(self.model.model().as_bytes());
        hasher.update(b"\0");
        hasher.update(intent.as_bytes());
        for source in pool {
            hasher.update(b"\0");
            hasher.update(source.entry_ref.as_bytes());
            hasher.update(source.text_sha256.as_bytes());
        }
        let key = format!("{:x}", hasher.finalize());
        self.selections
            .get_or_run(&key, !continuation, || self.judge_in_time(intent, pool))
            .await
            .map(|(outcome, _)| outcome)
            .ok_or_else(|| "wake focus expired or its evidence changed; start a fresh wake".into())
    }

    async fn judge_in_time(&self, intent: &str, pool: &[SemanticSource]) -> WakeFocusOutcome {
        let model = Arc::clone(&self.model);
        let (state, pool, excerpt_chars) = (
            format!("An agent is resuming this work: {intent}"),
            pool.to_vec(),
            self.excerpt_chars,
        );
        // Detached, so a judgement past the deadline still lands in the book
        // for the next wake.
        let judged = tokio::spawn(async move {
            let limit = pool.len();
            let kept = judge_passages(
                model.as_ref(),
                &state,
                "Does `passage` matter for resuming the work in the state?",
                &pool,
                excerpt_chars,
                MATTERS_AT,
                limit,
            )
            .await?;
            JudgedSelection::new(model.model().to_string(), kept)
                .map_err(|_| "invalid wake focus identities".to_string())
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
            Ok(Ok(selection)) => WakeFocusOutcome {
                selection: Some(selection),
                warning: None,
            },
            Ok(Err(error)) => unavailable(error),
            Err(_) => unavailable("the judgement task failed".into()),
        }
    }
}

fn unavailable(error: String) -> WakeFocusOutcome {
    WakeFocusOutcome {
        selection: None,
        warning: Some(format!("wake focus unavailable; ordinary wake: {error}")),
    }
}
