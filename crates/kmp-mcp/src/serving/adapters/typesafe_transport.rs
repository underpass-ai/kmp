use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};

use super::typesafe_api_key::TypeSafeApiKey;
use super::typesafe_batches::REQUEST_BYTES;
use super::typesafe_wire_response::TypeSafeWireResponse;
use crate::serving::judgement_failure::JudgementFailure;

const MAX_RETRIES: u32 = 2;
const MAX_RETRY_WAIT_SECS: u64 = 5;
/// How much of a refusal's body is read for its reason.
const REFUSAL_BYTES: usize = 4 * 1024;

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
                code @ (400 | 413 | 422) => {
                    let reason = refusal_reason(&bounded_body(&mut response).await);
                    return Err(JudgementFailure::refused(&match reason {
                        Some(reason) => format!("TypeSafe HTTP {code} {reason}"),
                        None => format!("TypeSafe HTTP {code}"),
                    }));
                }
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

/// At most [`REFUSAL_BYTES`] of a refusal's body; nothing when it cannot be
/// read.
async fn bounded_body(response: &mut reqwest::Response) -> Vec<u8> {
    let mut bytes = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        let room = REFUSAL_BYTES.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if bytes.len() >= REFUSAL_BYTES {
            break;
        }
    }
    bytes
}

/// The provider's reason for a refusal, as a code such as
/// `max_tokens_exceeded`: the first `code`, `type` or `error` string of the
/// body (at the top or inside `error`) that reads as a code, lowercase
/// letters, digits and `_`. Anything else in the body is never repeated: it
/// may echo the request.
fn refusal_reason(body: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<serde_json::Value>(body).ok()?;
    let is_code = |text: &str| {
        (1..=64).contains(&text.len())
            && text.starts_with(|c: char| c.is_ascii_lowercase())
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    let places = [&value, &value["error"]];
    places
        .iter()
        .flat_map(|place| ["code", "type", "error"].map(|field| &place[field]))
        .filter_map(serde_json::Value::as_str)
        .find(|text| is_code(text))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_names_its_code_and_nothing_else_of_the_body() {
        for body in [
            r#"{"error":"max_tokens_exceeded","detail":"request has 71234 tokens"}"#,
            r#"{"error":{"code":"max_tokens_exceeded","message":"too long: facts.f1 ..."}}"#,
            r#"{"code":"max_tokens_exceeded"}"#,
        ] {
            assert_eq!(
                refusal_reason(body.as_bytes()).as_deref(),
                Some("max_tokens_exceeded"),
                "{body}"
            );
        }
        for body in [
            r#"{"error":"The request echoes: Marta signed off"}"#,
            r#"{"detail":"max_tokens_exceeded"}"#,
            "not json",
            "",
        ] {
            assert_eq!(refusal_reason(body.as_bytes()), None, "{body}");
        }
    }
}
