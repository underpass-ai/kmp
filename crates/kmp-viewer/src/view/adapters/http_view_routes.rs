//! The browser's face of the view aggregate.
//!
//! Memory is read-only on the HTTP surface and stays that way. The one
//! exception is the view aggregate — where the human is looking — which the
//! browser reports back so an agent can see it and rebase instead of
//! yanking the loom out from under them. A camera position is not memory,
//! and POST is the honest method for changing one.

use std::time::Duration;

use crate::http::{HttpRequest, HttpResponse};
use crate::view::ViewRegistry;
use crate::view::adapters::view_error_status::view_error_response;
use crate::view::application::commands::{ApplyIntentCommand, OpenViewCommand};
use crate::view::application::dto::{TimeRangeDto, TraceSelectionDto, ViewIntentDto};
use crate::view::application::mappers::view_state_dto;
use crate::view::domain::DEFAULT_VIEW_ID;

/// How long a browser's long poll waits before answering with the state it
/// already had. Comfortably inside the request timeout, so a poll never
/// looks like a stuck client.
const VIEW_POLL_PATIENCE: Duration = Duration::from_secs(20);

/// The view state, or — with `since` — the next one: a long poll, so an
/// agent's intent reaches the screen without the browser spinning.
pub(crate) async fn view_get(request: &HttpRequest) -> HttpResponse {
    let id = request.param("id").unwrap_or(DEFAULT_VIEW_ID).to_string();
    let registry = ViewRegistry::shared();
    let state = match request
        .param("since")
        .and_then(|since| since.parse::<u64>().ok())
    {
        Some(since) => {
            registry
                .changed_since(Some(&id), since, VIEW_POLL_PATIENCE)
                .await
        }
        None => registry.view_state(Some(&id)),
    };
    match state {
        Some(state) => HttpResponse::json(&view_state_dto(&state)),
        None => HttpResponse::error(404, "no view under that id — open one first"),
    }
}

pub(crate) fn view_open(request: &HttpRequest) -> HttpResponse {
    let expected = match request.param("expected_revision") {
        Some(value) => match value.parse::<u64>() {
            Ok(revision) => Some(revision),
            Err(_) => {
                return HttpResponse::error(
                    400,
                    "parameter `expected_revision` must be an unsigned integer",
                );
            }
        },
        None => None,
    };
    let command = OpenViewCommand {
        view_id: request.param("id").map(str::to_string),
        about: request.param("about").map(str::to_string),
        expected_revision: expected,
        actor: "human".to_string(),
        explanation: Some("opened a different about".to_string()),
    };
    match ViewRegistry::shared().open_view(command) {
        Ok(state) => HttpResponse::json(&view_state_dto(&state)),
        Err(error) => view_error_response(&error),
    }
}

/// Where the human is looking now. Reported by the browser so the agent's
/// `kmp_view_get_state` answers with the truth rather than with whatever it
/// last asked for.
pub(crate) fn view_report(request: &HttpRequest) -> HttpResponse {
    let id = request.param("id").unwrap_or(DEFAULT_VIEW_ID).to_string();
    let registry = ViewRegistry::shared();
    if registry.view_state(Some(&id)).is_none() {
        registry.ensure_open(Some(&id), request.param("about").map(str::to_string));
    }
    let trace = match (request.param("trace_from"), request.param("trace_to")) {
        (Some(from), Some(to)) => Some(Some(TraceSelectionDto {
            from: from.to_string(),
            to: to.to_string(),
        })),
        _ => Some(None),
    };
    let intent = ViewIntentDto {
        about: request.param("about").map(str::to_string),
        clock: request.param("clock").map(str::to_string),
        // Only the window: the refs an agent asked to frame are its intent,
        // and a person panning does not retract it.
        focus_window: Some(TimeRangeDto {
            from: request.param("from").map(str::to_string),
            to: request.param("to").map(str::to_string),
        }),
        focus: None,
        selection: Some(request.param("selection").map(str::to_string)),
        trace,
        search: Some(request.param("search").map(str::to_string)),
        projection: None,
    };
    let command = ApplyIntentCommand {
        view_id: Some(id),
        intent,
        actor: "human".to_string(),
        ..ApplyIntentCommand::default()
    };
    match registry.apply_intent(command) {
        Ok(applied) => HttpResponse::json(&view_state_dto(&applied.state)),
        Err(error) => view_error_response(&error),
    }
}

pub(crate) fn view_undo(request: &HttpRequest) -> HttpResponse {
    let id = request.param("id").unwrap_or(DEFAULT_VIEW_ID);
    match ViewRegistry::shared().undo(Some(id), "human") {
        Ok(state) => HttpResponse::json(&view_state_dto(&state)),
        Err(error) => view_error_response(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(path: &str, query: &[(&str, &str)]) -> HttpRequest {
        HttpRequest {
            method: "POST".to_string(),
            path: path.to_string(),
            query: query
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            host: None,
            cookie: None,
        }
    }

    fn body(response: HttpResponse) -> serde_json::Value {
        serde_json::from_slice(&response.body).expect("a JSON body")
    }

    #[tokio::test]
    async fn a_view_nobody_opened_is_refused_not_invented() {
        let response = view_get(&request("/api/view", &[("id", "routes-nobody")])).await;
        assert_eq!(response.status, 404);
        let undone = view_undo(&request("/api/view/undo", &[("id", "routes-nobody")]));
        assert_eq!(undone.status, 404);
    }

    #[tokio::test]
    async fn the_browser_opens_reports_polls_and_undoes_over_http() {
        let malformed = view_open(&request(
            "/api/view/open",
            &[("id", "routes-loom"), ("expected_revision", "not-a-number")],
        ));
        assert_eq!(malformed.status, 400);

        let opened = body(view_open(&request(
            "/api/view/open",
            &[("id", "routes-loom"), ("about", "about:routes")],
        )));
        assert_eq!(opened["view_id"], "routes-loom");
        assert_eq!(opened["about"], "about:routes");

        let reported = body(view_report(&request(
            "/api/view/report",
            &[
                ("id", "routes-loom"),
                ("about", "about:routes"),
                ("clock", "observed"),
                ("from", "2026-08-31T16:49:00Z"),
                ("to", "2026-08-31T17:39:00Z"),
                ("search", "attempt-000005"),
                ("selection", "decision:one"),
                ("trace_from", "decision:one"),
                ("trace_to", "success:two"),
            ],
        )));
        assert_eq!(reported["clock"], "observed");
        assert_eq!(reported["search"], "attempt-000005");
        assert_eq!(reported["trace"]["to"], "success:two");
        assert_eq!(reported["last_change"]["actor"], "human");

        let since = reported["view_revision"].as_u64().expect("revision");
        let polled = view_get(&request(
            "/api/view",
            &[("id", "routes-loom"), ("since", &(since - 1).to_string())],
        ))
        .await;
        assert_eq!(polled.status, 200, "a newer revision answers immediately");

        let read = body(view_get(&request("/api/view", &[("id", "routes-loom")])).await);
        assert_eq!(read["view_revision"].as_u64(), Some(since));

        let undone = body(view_undo(&request(
            "/api/view/undo",
            &[("id", "routes-loom")],
        )));
        assert!(undone["view_revision"].as_u64() > Some(since));
        let undo_actor = undone["last_change"]["actor"].as_str();
        assert_eq!(undo_actor, Some("human"));
    }

    /// A report onto a view nobody opened first opens it — the browser must
    /// be able to speak before the agent does.
    #[tokio::test]
    async fn a_report_onto_a_cold_loom_opens_it_first() {
        let reported = body(view_report(&request(
            "/api/view/report",
            &[("id", "routes-cold"), ("about", "about:cold")],
        )));
        assert_eq!(reported["view_id"], "routes-cold");
        assert_eq!(reported["about"], "about:cold");
    }
}
