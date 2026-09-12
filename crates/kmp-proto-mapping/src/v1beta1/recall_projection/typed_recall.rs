//! Projecting a typed wake or ask response: render it, bound it, and read
//! the bounded JSON back onto the proto message the caller receives.

use kmp_application::queries::cl100k_estimator::Cl100kEstimator;
use kmp_proto::v1beta1::{AskRequest, AskResponse, WakeRequest, WakeResponse};
use serde_json::Value;

use super::projection_error::RecallProjectionError;
use super::projection_outcome::ProjectionOutcome;
use super::recall_output::project_recall_output_typed;
use super::request_arguments::{ask_arguments, wake_arguments};
use super::response_value::{ask_value, wake_value};
use super::typed_response::{apply_ask_value, apply_wake_value};

pub fn project_wake_response(
    mut response: WakeResponse,
    request: &WakeRequest,
) -> Result<WakeResponse, RecallProjectionError> {
    response.dimension_selection = Some(request.dimensions.clone().unwrap_or_default());
    let arguments = wake_arguments(request);
    project_typed_recall(response, arguments, 1_600, wake_value, apply_wake_value)
}

pub fn project_ask_response(
    response: AskResponse,
    request: &AskRequest,
) -> Result<AskResponse, RecallProjectionError> {
    let arguments = ask_arguments(request);
    project_typed_recall(response, arguments, 2_400, ask_value, apply_ask_value)
}

fn project_typed_recall<T>(
    response: T,
    arguments: Value,
    default_tokens: u32,
    render: fn(&T) -> Value,
    apply: fn(T, &Value) -> T,
) -> Result<T, RecallProjectionError> {
    let value = render(&response);
    match project_recall_output_typed(value, &arguments, default_tokens, Cl100kEstimator::shared())?
    {
        ProjectionOutcome::Projected(value) => Ok(apply(response, &value)),
        ProjectionOutcome::CoreTooLarge => Err(RecallProjectionError::CoreTooLarge),
    }
}
