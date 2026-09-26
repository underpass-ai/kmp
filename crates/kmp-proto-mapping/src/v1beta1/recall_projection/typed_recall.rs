//! Projecting a typed wake or ask response: render it, bound it, and read
//! the bounded JSON back onto the proto message the caller receives.
//!
//! Rendering is split from cutting so a caller that pages through one read
//! (a frozen continuation) renders it once and cuts every page from the same
//! JSON: `project_*_response` is exactly `render_*` then `project_rendered_*`.

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
    response: WakeResponse,
    request: &WakeRequest,
) -> Result<WakeResponse, RecallProjectionError> {
    let (response, rendered) = render_wake(response, request);
    project_rendered_wake(response, rendered, request)
}

pub fn project_ask_response(
    response: AskResponse,
    request: &AskRequest,
) -> Result<AskResponse, RecallProjectionError> {
    let rendered = render_ask(&response);
    project_rendered_ask(response, rendered, request)
}

/// The wake as the projection reads it, with the request's dimension
/// selection echoed onto the response the pages are applied to.
pub fn render_wake(mut response: WakeResponse, request: &WakeRequest) -> (WakeResponse, Value) {
    response.dimension_selection = Some(request.dimensions.clone().unwrap_or_default());
    let rendered = wake_value(&response);
    (response, rendered)
}

/// The ask as the projection reads it.
pub fn render_ask(response: &AskResponse) -> Value {
    ask_value(response)
}

/// Cut one page of a wake already rendered by [`render_wake`] for a request
/// with the same dimension selection.
pub fn project_rendered_wake(
    response: WakeResponse,
    rendered: Value,
    request: &WakeRequest,
) -> Result<WakeResponse, RecallProjectionError> {
    project_rendered(
        response,
        rendered,
        wake_arguments(request),
        1_600,
        apply_wake_value,
    )
}

/// Cut one page of an ask already rendered by [`render_ask`].
pub fn project_rendered_ask(
    response: AskResponse,
    rendered: Value,
    request: &AskRequest,
) -> Result<AskResponse, RecallProjectionError> {
    project_rendered(
        response,
        rendered,
        ask_arguments(request),
        2_400,
        apply_ask_value,
    )
}

fn project_rendered<T>(
    response: T,
    rendered: Value,
    arguments: Value,
    default_tokens: u32,
    apply: fn(T, &Value) -> T,
) -> Result<T, RecallProjectionError> {
    match project_recall_output_typed(
        rendered,
        &arguments,
        default_tokens,
        Cl100kEstimator::shared(),
    )? {
        ProjectionOutcome::Projected(value) => Ok(apply(response, &value)),
        ProjectionOutcome::CoreTooLarge => Err(RecallProjectionError::CoreTooLarge),
    }
}
