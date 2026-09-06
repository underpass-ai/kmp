//! Loopback transport for an explicit, revision-checked human handoff.

use crate::http::{HttpRequest, HttpResponse};
use crate::view::ViewRegistry;
use crate::view::adapters::view_error_status::view_error_response;
use crate::view::application::mappers::view_state_dto;

pub(crate) fn view_take_control(request: &HttpRequest) -> HttpResponse {
    let Some(expected) = request
        .param("expected_revision")
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return HttpResponse::error(400, "expected_revision must be an unsigned integer");
    };
    match ViewRegistry::shared().take_control(request.param("id"), expected) {
        Ok(state) => HttpResponse::json(&view_state_dto(&state)),
        Err(error) => view_error_response(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::application::commands::ApplyIntentCommand;
    use crate::view::application::dto::ViewIntentDto;

    fn request(revision: Option<&str>) -> HttpRequest {
        HttpRequest {
            method: "POST".into(),
            path: "/api/view/take-control".into(),
            query: [("id", "http-handoff")]
                .into_iter()
                .chain(revision.map(|value| ("expected_revision", value)))
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
            host: None,
            cookie: None,
        }
    }

    #[test]
    fn the_http_button_requires_the_seen_revision_and_preserves_the_frame() {
        assert_eq!(view_take_control(&request(None)).status, 400);
        assert_eq!(view_take_control(&request(Some("bad"))).status, 400);
        assert_eq!(view_take_control(&request(Some("1"))).status, 404);
        let registry = ViewRegistry::shared();
        registry.ensure_open(Some("http-handoff"), Some("about:test".into()));
        let moved = registry
            .apply_intent(ApplyIntentCommand {
                view_id: Some("http-handoff".into()),
                actor: "agent:test".into(),
                intent: ViewIntentDto {
                    search: Some(Some("proof".into())),
                    ..ViewIntentDto::default()
                },
                ..ApplyIntentCommand::default()
            })
            .expect("successful test view operation")
            .state;
        assert_eq!(view_take_control(&request(Some("1"))).status, 409);
        let response = view_take_control(&request(Some(&moved.view_revision.value().to_string())));
        assert_eq!(response.status, 200);
        let body: serde_json::Value =
            serde_json::from_slice(&response.body).expect("successful test view operation");
        assert_eq!(body["last_change"]["actor"], "human");
        assert_eq!(body["search"], "proof");
    }
}
