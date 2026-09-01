//! The view's error vocabulary, translated to HTTP status codes.

use crate::http::HttpResponse;
use crate::view::domain::ViewError;

/// Maps a refusal to a status without leaking anything the message itself
/// does not already say.
pub(crate) fn view_error_response(error: &ViewError) -> HttpResponse {
    match error {
        ViewError::UnknownView(id) => HttpResponse::error(404, &format!("no view under `{id}`")),
        ViewError::Conflict {
            expected, actual, ..
        } => HttpResponse::error(
            409,
            &format!("the view moved on: expected revision {expected}, it is at {actual}"),
        ),
        ViewError::IdempotencyConflict { key } => HttpResponse::error(
            409,
            &format!(
                "idempotency key '{}' was already accepted with different content",
                key.as_str()
            ),
        ),
        ViewError::Invalid(message) => HttpResponse::error(400, message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::domain::{AboutId, IdempotencyKey, ViewId, ViewRevision, ViewState};

    #[test]
    fn every_refusal_gets_the_status_its_nature_deserves() {
        let missing = view_error_response(&ViewError::UnknownView("t".into()));
        assert_eq!(missing.status, 404);

        let conflict = view_error_response(&ViewError::Conflict {
            expected: ViewRevision::from(3),
            actual: ViewRevision::from(5),
            current: Box::new(ViewState::opened(
                ViewId::from("t"),
                Some(AboutId::new("about:x")),
            )),
        });
        assert_eq!(conflict.status, 409);
        let body = String::from_utf8(conflict.body).expect("utf8 body");
        assert!(body.contains("expected revision 3"), "{body}");
        assert!(body.contains("it is at 5"), "{body}");

        let collision = view_error_response(&ViewError::IdempotencyConflict {
            key: IdempotencyKey::new("same"),
        });
        assert_eq!(collision.status, 409);

        let invalid = view_error_response(&ViewError::Invalid("not a clock".into()));
        assert_eq!(invalid.status, 400);
    }
}
