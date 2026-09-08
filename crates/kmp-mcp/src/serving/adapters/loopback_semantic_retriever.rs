use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

use kmp_proto_mapping::v1beta1::{SemanticCandidateRanking, SemanticSource};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::{
    semantic_rank_response::SemanticRankResponse,
    semantic_retriever_config::SemanticRetrieverConfig,
};
use crate::serving::ports::semantic_candidate_provider::SemanticCandidateProvider;
use crate::serving::semantic_retrieval_outcome::SemanticRetrievalOutcome;

/// Local, explicitly configured semantic retrieval. Frozen outcomes include
/// failures, so a continuation cannot silently perform a different selection.
pub(super) struct LoopbackSemanticRetriever {
    endpoint: reqwest::Url,
    model_revision: String,
    client: reqwest::Client,
    selections: Mutex<BTreeMap<String, SemanticRetrievalOutcome>>,
}

impl LoopbackSemanticRetriever {
    pub(super) fn load(
        data_dir: &Path,
    ) -> Result<Option<Arc<dyn SemanticCandidateProvider>>, String> {
        let path = data_dir.join("semantic-retrieval.json");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= 8192 => bytes,
            Ok(_) => return Err("semantic configuration exceeds 8192 bytes".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("cannot read semantic configuration".into()),
        };
        let config: SemanticRetrieverConfig =
            serde_json::from_slice(&bytes).map_err(|_| "invalid semantic configuration")?;
        let endpoint = config.validate()?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|_| "cannot create local semantic client")?;
        Ok(Some(Arc::new(Self {
            endpoint,
            model_revision: config.model_revision,
            client,
            selections: Mutex::new(BTreeMap::new()),
        })))
    }

    async fn request(
        &self,
        question: &str,
        body: Vec<u8>,
    ) -> Result<SemanticCandidateRanking, String> {
        if body.len() > 16 * 1024 * 1024 {
            return Err("semantic source batch exceeds 16 MiB".into());
        }
        let mut response = self
            .client
            .post(self.endpoint.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|_| "local semantic endpoint unavailable or timed out")?;
        if !response.status().is_success() {
            return Err(format!(
                "local semantic endpoint returned {}",
                response.status()
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "semantic response interrupted")?
        {
            if bytes.len() + chunk.len() > 512 * 1024 {
                return Err("semantic response exceeds 512 KiB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let response: SemanticRankResponse =
            serde_json::from_slice(&bytes).map_err(|_| "invalid semantic response")?;
        if response.model_revision != self.model_revision
            || response.question_sha256 != format!("{:x}", Sha256::digest(question.as_bytes()))
        {
            return Err("semantic response model or question mismatch".into());
        }
        let ranking =
            SemanticCandidateRanking::new(response.model_revision, question, response.candidates)
                .map_err(|_| "invalid semantic candidate identities".to_string())?;
        match response.lexical_candidates {
            Some(lexical) => ranking
                .with_lexical_candidates(lexical)
                .map_err(|_| "invalid lexical candidate identities".to_string()),
            None => Ok(ranking),
        }
    }
}

impl SemanticCandidateProvider for LoopbackSemanticRetriever {
    fn rank<'a>(
        &'a self,
        question: &'a str,
        sources: &'a [SemanticSource],
        continuation: bool,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<SemanticRetrievalOutcome, String>> + Send + 'a>,
    > {
        Box::pin(async move {
            let body = serde_json::to_vec(&json!({"question": question, "sources": sources,
                "model_revision": self.model_revision, "top_k": 100}))
            .map_err(|_| "cannot encode semantic request")?;
            let key = format!("{:x}", Sha256::digest(&body));
            let mut selections = self.selections.lock().await;
            if let Some(outcome) = selections.get(&key) {
                if continuation || outcome.ranking.is_some() {
                    return Ok(outcome.clone());
                }
            } else if continuation {
                return Err(
                    "semantic selection expired or its source snapshot changed; start a fresh ask"
                        .into(),
                );
            }
            let outcome = if sources.is_empty() {
                SemanticRetrievalOutcome {
                    ranking: Some(
                        SemanticCandidateRanking::new(
                            self.model_revision.clone(),
                            question,
                            Vec::new(),
                        )
                        .map_err(|_| "invalid model revision")?,
                    ),
                    warning: None,
                }
            } else {
                match self.request(question, body).await {
                    Ok(ranking) => SemanticRetrievalOutcome {
                        ranking: Some(ranking),
                        warning: None,
                    },
                    Err(error) => SemanticRetrievalOutcome {
                        ranking: None,
                        warning: Some(format!(
                            "semantic retrieval unavailable; using ordinary retrieval: {error}"
                        )),
                    },
                }
            };
            if selections.len() >= 64 {
                selections.pop_first();
            }
            selections.insert(key, outcome.clone());
            Ok(outcome)
        })
    }
}
