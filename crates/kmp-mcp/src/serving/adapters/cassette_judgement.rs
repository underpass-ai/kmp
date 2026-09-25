use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::typesafe_request_body::typesafe_request_body;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

const SCHEMA: &str = "kmp.typesafe.cassette.v1";

/// Recorded judgements, for evaluation that must be repeatable. Replay answers
/// only what was recorded for exactly this model and request, and fails
/// loudly otherwise; record asks the real model once per new request and
/// keeps the answer. The file never holds a key.
pub(super) struct CassetteJudgement {
    inner: Option<Arc<dyn JudgementModel>>,
    model: String,
    path: PathBuf,
    entries: Mutex<BTreeMap<String, JudgementResponse>>,
}

impl CassetteJudgement {
    pub(super) fn replay(path: &Path, model: String) -> Result<Self, String> {
        let entries = read_entries(path, &model)?
            .ok_or_else(|| format!("judgement cassette `{}` does not exist", path.display()))?;
        Ok(Self {
            inner: None,
            model,
            path: path.to_path_buf(),
            entries: Mutex::new(entries),
        })
    }

    pub(super) fn record(path: &Path, inner: Arc<dyn JudgementModel>) -> Result<Self, String> {
        let model = inner.model().to_string();
        let entries = read_entries(path, &model)?.unwrap_or_default();
        Ok(Self {
            inner: Some(inner),
            model,
            path: path.to_path_buf(),
            entries: Mutex::new(entries),
        })
    }

    fn key(&self, request: &JudgementRequest) -> String {
        let body = typesafe_request_body(&self.model, &request.state, &request.questions);
        format!("{:x}", Sha256::digest(body.to_string().as_bytes()))
    }

    fn persist(&self, entries: &BTreeMap<String, JudgementResponse>) -> Result<(), String> {
        let document = json!({"schema": SCHEMA, "model": self.model, "entries": entries});
        let text = serde_json::to_string_pretty(&document)
            .map_err(|_| "cannot encode judgement cassette")?;
        let staging = self.path.with_extension("tmp");
        std::fs::write(&staging, text + "\n")
            .and_then(|()| std::fs::rename(&staging, &self.path))
            .map_err(|error| {
                format!(
                    "cannot write judgement cassette `{}`: {error}",
                    self.path.display()
                )
            })
    }
}

fn read_entries(
    path: &Path,
    model: &str,
) -> Result<Option<BTreeMap<String, JudgementResponse>>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "cannot read judgement cassette `{}`: {error}",
                path.display()
            ));
        }
    };
    let document: Value =
        serde_json::from_str(&text).map_err(|_| "judgement cassette is not JSON")?;
    if document["schema"] != SCHEMA {
        return Err(format!("judgement cassette schema must be {SCHEMA}"));
    }
    if document["model"] != model {
        return Err(format!(
            "judgement cassette was recorded for `{}`, not `{model}`",
            document["model"].as_str().unwrap_or_default()
        ));
    }
    serde_json::from_value(document["entries"].clone())
        .map(Some)
        .map_err(|_| "judgement cassette entries are malformed".into())
}

impl JudgementModel for CassetteJudgement {
    fn model(&self) -> &str {
        &self.model
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let key = self.key(request);
            if let Some(recorded) = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&key)
            {
                return Ok(recorded.clone());
            }
            let Some(inner) = &self.inner else {
                return Err(format!(
                    "judgement `{key}` is not in the cassette; record it with KMP_TYPESAFE_CASSETTE_MODE=record"
                ));
            };
            let response = inner.evaluate(request).await?;
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            // Another recorder (another store in the same evaluation) may
            // have written since this one loaded; keep what it recorded.
            if let Some(on_disk) = read_entries(&self.path, &self.model)? {
                for (recorded_key, recorded) in on_disk {
                    entries.entry(recorded_key).or_insert(recorded);
                }
            }
            entries.insert(key, response.clone());
            self.persist(&entries)?;
            Ok(response)
        })
    }
}
