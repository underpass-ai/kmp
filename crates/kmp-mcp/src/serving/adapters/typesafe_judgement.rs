use std::{collections::BTreeMap, path::Path, pin::Pin, sync::Arc, time::Duration};

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};

use super::typesafe_api_key::TypeSafeApiKey;
use super::typesafe_batches::{REQUEST_BYTES, typesafe_batches};
use super::typesafe_config::TypeSafeConfig;
use super::typesafe_request_body::typesafe_request_body;
use super::typesafe_wire_response::TypeSafeWireResponse;
use crate::serving::judgement_request::JudgementRequest;
use crate::serving::judgement_response::JudgementResponse;
use crate::serving::ports::judgement_model::JudgementModel;

const MAX_RETRIES: u32 = 2;
const MAX_RETRY_WAIT_SECS: u64 = 5;

/// TypeSafe Jev behind an explicit per-store opt-in. Answers are validated
/// against the questions sent; failures carry no key and no provider body.
#[derive(Debug)]
pub(super) struct TypeSafeJudgement {
    endpoint: reqwest::Url,
    model: String,
    key: TypeSafeApiKey,
    client: reqwest::Client,
}

impl TypeSafeJudgement {
    pub(super) fn load(
        data_dir: &Path,
        key: Option<String>,
    ) -> Result<Option<Arc<dyn JudgementModel>>, String> {
        let bytes = match std::fs::read(data_dir.join("typesafe.json")) {
            Ok(bytes) if bytes.len() <= 8192 => bytes,
            Ok(_) => return Err("TypeSafe configuration exceeds 8192 bytes".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("cannot read TypeSafe configuration".into()),
        };
        let config: TypeSafeConfig =
            serde_json::from_slice(&bytes).map_err(|_| "invalid TypeSafe configuration")?;
        let (endpoint, timeout) = config.validate()?;
        let key = TypeSafeApiKey::from_env(key)?;
        Ok(Some(Arc::new(Self::new(
            endpoint,
            config.model,
            key,
            timeout,
        )?)))
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
            endpoint,
            model,
            key,
            client,
        })
    }

    async fn send(&self, body: Vec<u8>) -> Result<TypeSafeWireResponse, String> {
        if body.len() > REQUEST_BYTES {
            return Err("TypeSafe request exceeds 256 KiB".into());
        }
        let mut attempt = 0;
        loop {
            let mut response = self
                .client
                .post(self.endpoint.clone())
                .header(AUTHORIZATION, self.key.header())
                .header(CONTENT_TYPE, "application/json")
                .body(body.clone())
                .send()
                .await
                .map_err(|error| {
                    if error.is_timeout() {
                        "TypeSafe timed out"
                    } else {
                        "TypeSafe unavailable"
                    }
                })?;
            let status = response.status();
            if status == StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RETRIES {
                let wait = response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .unwrap_or(1)
                    .min(MAX_RETRY_WAIT_SECS);
                tokio::time::sleep(Duration::from_secs(wait)).await;
                attempt += 1;
                continue;
            }
            match status.as_u16() {
                200..=299 => {}
                401 | 403 => {
                    return Err(format!(
                        "TypeSafe rejected the API key ({})",
                        status.as_u16()
                    ));
                }
                429 => return Err("TypeSafe rate limit persisted after retries".into()),
                code => return Err(format!("TypeSafe returned HTTP {code}")),
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|error| {
                if error.is_timeout() {
                    "TypeSafe timed out"
                } else {
                    "TypeSafe response interrupted"
                }
            })? {
                if bytes.len() + chunk.len() > REQUEST_BYTES {
                    return Err("TypeSafe response exceeds 256 KiB".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            return serde_json::from_slice(&bytes).map_err(|_| "invalid TypeSafe response".into());
        }
    }
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
            let mut merged = JudgementResponse {
                model: self.model.clone(),
                answers: BTreeMap::new(),
                input_tokens: 0,
                requests: 0,
            };
            for batch in typesafe_batches(request)? {
                let body =
                    serde_json::to_vec(&typesafe_request_body(&self.model, &request.state, &batch))
                        .map_err(|_| "cannot encode TypeSafe request")?;
                let part = self.send(body).await?.into_response(&self.model, &batch)?;
                merged.answers.extend(part.answers);
                merged.input_tokens += part.input_tokens;
                merged.requests += 1;
            }
            Ok(merged)
        })
    }
}
