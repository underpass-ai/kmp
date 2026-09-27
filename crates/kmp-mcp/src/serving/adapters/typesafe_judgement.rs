use std::{path::Path, pin::Pin, sync::Arc, time::Duration};

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use super::typesafe_api_key::TypeSafeApiKey;
use super::typesafe_batches::TypeSafeBatches;
use super::typesafe_config::TypeSafeConfig;
use super::typesafe_request_body::typesafe_request_body;
use super::typesafe_transport::TypeSafeTransport;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

/// Provider requests in flight at once, over every caller of the process
/// (DESIGN L4 4b: 3–4, well inside 1,200 requests a minute).
pub(super) const CONCURRENT_REQUESTS: usize = 4;

/// TypeSafe Jev behind an explicit per-store opt-in. Answers are validated
/// against the questions sent; failures carry no key and no provider body.
/// The batches of one judgement go out in parallel, at most
/// `CONCURRENT_REQUESTS` at a time for the whole process.
#[derive(Debug)]
pub(super) struct TypeSafeJudgement {
    model: String,
    transport: Arc<TypeSafeTransport>,
    permits: Arc<Semaphore>,
}

impl TypeSafeJudgement {
    pub(super) fn load(
        data_dir: &Path,
        key: Option<String>,
    ) -> Result<Option<Arc<dyn JudgementModel>>, String> {
        Self::load_with(data_dir, key, TypeSafeApiKey::load)
    }

    /// `load` with the key resolution given, so tests do not depend on a key
    /// file the machine happens to hold.
    pub(super) fn load_with(
        data_dir: &Path,
        key: Option<String>,
        resolve: impl FnOnce(Option<String>) -> Result<TypeSafeApiKey, String>,
    ) -> Result<Option<Arc<dyn JudgementModel>>, String> {
        let Some(config) = read_config(data_dir)? else {
            return Ok(None);
        };
        let (endpoint, timeout) = config.validate()?;
        let key = resolve(key)?;
        Ok(Some(Arc::new(Self::new(
            endpoint,
            config.model,
            key,
            timeout,
        )?)))
    }

    /// The pinned model the store opted into, validated like a live load but
    /// without a key: what a replayed cassette must have been recorded for.
    pub(super) fn configured_model(data_dir: &Path) -> Result<Option<String>, String> {
        let Some(config) = read_config(data_dir)? else {
            return Ok(None);
        };
        config.validate()?;
        Ok(Some(config.model))
    }

    pub(super) fn new(
        endpoint: reqwest::Url,
        model: String,
        key: TypeSafeApiKey,
        timeout: Duration,
    ) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(timeout)
            .build()
            .map_err(|_| "cannot create TypeSafe client")?;
        Ok(Self {
            model,
            transport: Arc::new(TypeSafeTransport {
                endpoint,
                key,
                client,
            }),
            permits: Arc::new(Semaphore::new(CONCURRENT_REQUESTS)),
        })
    }
}

fn read_config(data_dir: &Path) -> Result<Option<TypeSafeConfig>, String> {
    let bytes = match std::fs::read(data_dir.join("typesafe.json")) {
        Ok(bytes) if bytes.len() <= 8192 => bytes,
        Ok(_) => return Err("TypeSafe configuration exceeds 8192 bytes".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("cannot read TypeSafe configuration".into()),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "invalid TypeSafe configuration".into())
}

impl JudgementModel for TypeSafeJudgement {
    fn model(&self) -> &str {
        &self.model
    }

    fn evaluate<'a>(
        &'a self,
        request: &'a JudgementRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<JudgementResponse, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut merged = JudgementResponse::empty(&self.model);
            let mut sent = JoinSet::new();
            let TypeSafeBatches { state, batches, .. } = TypeSafeBatches::plan(request)?;
            for (index, batch) in batches.iter().enumerate() {
                let body = serde_json::to_vec(&typesafe_request_body(&self.model, &state, batch))
                    .map_err(|_| "cannot encode TypeSafe request")?;
                let transport = Arc::clone(&self.transport);
                let permits = Arc::clone(&self.permits);
                sent.spawn(async move {
                    let _permit = permits
                        .acquire_owned()
                        .await
                        .map_err(|_| "TypeSafe requests closed".to_string())?;
                    Ok::<_, String>((index, transport.send(body).await))
                });
            }
            let mut replies = (0..batches.len()).map(|_| None).collect::<Vec<_>>();
            while let Some(joined) = sent.join_next().await {
                let (index, reply) = joined.map_err(|_| "TypeSafe request task failed")??;
                replies[index] = Some(reply);
            }
            // Merged in batch order, and the first failing batch in that
            // order names the failure, however the replies interleaved.
            for (batch, reply) in batches.iter().zip(replies) {
                let reply = reply.ok_or("TypeSafe request task lost")?;
                let part = reply?.into_response(&self.model, batch)?;
                merged.answers.extend(part.answers);
                merged.input_tokens += part.input_tokens;
                merged.requests += 1;
            }
            Ok(merged)
        })
    }
}
