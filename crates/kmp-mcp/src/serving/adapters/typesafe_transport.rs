use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};

use super::typesafe_api_key::TypeSafeApiKey;
use super::typesafe_batches::REQUEST_BYTES;
use super::typesafe_wire_response::TypeSafeWireResponse;

const MAX_RETRIES: u32 = 2;
const MAX_RETRY_WAIT_SECS: u64 = 5;

/// One HTTP exchange with TypeSafe: the bearer key, bounded bodies, and a
/// 429 retried after the delay the provider announces (`Retry-After`, at
/// most five seconds, twice). Failures carry no key and no provider body.
#[derive(Debug)]
pub(super) struct TypeSafeTransport {
    pub(super) endpoint: reqwest::Url,
    pub(super) key: TypeSafeApiKey,
    pub(super) client: reqwest::Client,
}

impl TypeSafeTransport {
    pub(super) async fn send(&self, body: Vec<u8>) -> Result<TypeSafeWireResponse, String> {
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
