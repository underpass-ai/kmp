use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use kmp_proto_mapping::v1beta1::{JudgedSelection, SemanticSource};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::passage_judgement::judge_passages;
use super::rerank_config::RerankConfig;
use crate::serving::ports::judgement_model::JudgementModel;
use crate::serving::wake_focus_outcome::WakeFocusOutcome;

const KEPT: usize = 64;
/// Evidence judged less likely than this to matter for the intent is
/// withheld from the packet.
const MATTERS_AT: f64 = 0.5;

/// A wake focused on what the caller says it is resuming. One yes/no
/// question per admitted evidence entry — does it matter for this intent? —
/// behind its own opt-in, `wake-focus.json`, on top of `typesafe.json`.
/// Frozen per intent, pool and model so continuation pages agree.
pub(super) struct WakeFocusJudge {
    model: Arc<dyn JudgementModel>,
    pool_size: usize,
    excerpt_chars: usize,
    selections: Mutex<BTreeMap<String, WakeFocusOutcome>>,
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
                selections: Mutex::new(BTreeMap::new()),
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
        let mut selections = self.selections.lock().await;
        if let Some(outcome) = selections.get(&key) {
            return Ok(outcome.clone());
        }
        if continuation {
            return Err("wake focus expired or its evidence changed; start a fresh wake".into());
        }
        let outcome = match judge_passages(
            self.model.as_ref(),
            &format!("An agent is resuming this work: {intent}"),
            "Does `passage` matter for resuming the work in the state?",
            pool,
            self.excerpt_chars,
            MATTERS_AT,
            pool.len(),
        )
        .await
        .and_then(|kept| {
            JudgedSelection::new(self.model.model().to_string(), kept)
                .map_err(|_| "invalid wake focus identities".to_string())
        }) {
            Ok(selection) => WakeFocusOutcome {
                selection: Some(selection),
                warning: None,
            },
            Err(error) => WakeFocusOutcome {
                selection: None,
                warning: Some(format!("wake focus unavailable; ordinary wake: {error}")),
            },
        };
        if selections.len() >= KEPT {
            selections.pop_first();
        }
        selections.insert(key, outcome.clone());
        Ok(outcome)
    }
}
