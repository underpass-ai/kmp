//! The key one judgement request is recorded and reported under.

use sha2::{Digest, Sha256};

use super::typesafe_batches::TypeSafeBatches;
use super::typesafe_request_body::typesafe_request_body;
use crate::serving::judgement_request::JudgementRequest;

/// The cassette key of one request: the sha256 of the provider body it
/// would send whole, with the state the provider reads (cut when it does not
/// fit a request beside its largest question), so an answer recorded for the
/// whole state never answers a cut one. Telemetry reports it so a line joins
/// its cassette entry.
pub(super) fn judgement_key(model: &str, request: &JudgementRequest) -> String {
    let state = TypeSafeBatches::wire_state(request);
    let body = typesafe_request_body(model, &state, &request.questions);
    format!("{:x}", Sha256::digest(body.to_string().as_bytes()))
}

